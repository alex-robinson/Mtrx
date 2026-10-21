//! The apps' path through the bindings, against the local Synapse from
//! `testserver/`. Calls are awaited without a tokio runtime, as Swift and
//! Kotlin do.
//!
//! ```sh
//! testserver/run.sh
//! cargo test -p mtrx-ffi --test integration -- --ignored
//! ```

#![recursion_limit = "256"]

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

use futures_executor::block_on;
use mtrx_core::{
    Attachment, NewRoom, Profile, SecretStore,
    test_support::{PASSWORD, homeserver, register, unique_name},
};
use mtrx_ffi::{
    ConnectionListener, ConnectionState, EventContent, MediaKind, Mtrx, RoomListListener,
    RoomListState, SecretBackend, SecretError, TimelineItem, TimelineItemContent, TimelineListener,
};

const TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Default)]
struct Memory(Mutex<HashMap<String, String>>);

impl SecretBackend for Memory {
    fn load(&self, key: String) -> Result<Option<String>, SecretError> {
        Ok(self.0.lock().unwrap().get(&key).cloned())
    }

    fn save(&self, key: String, value: String) -> Result<(), SecretError> {
        self.0.lock().unwrap().insert(key, value);
        Ok(())
    }

    fn delete(&self, key: String) -> Result<(), SecretError> {
        self.0.lock().unwrap().remove(&key);
        Ok(())
    }
}

/// A listener that forwards what it gets.
struct Forward<T>(Mutex<mpsc::Sender<T>>);

fn forward<T>() -> (Arc<Forward<T>>, mpsc::Receiver<T>) {
    let (tx, rx) = mpsc::channel();
    (Arc::new(Forward(Mutex::new(tx))), rx)
}

impl<T> Forward<T> {
    fn send(&self, value: T) {
        self.0.lock().unwrap().send(value).ok();
    }
}

impl RoomListListener for Forward<RoomListState> {
    fn on_update(&self, state: RoomListState) {
        self.send(state);
    }
}

impl TimelineListener for Forward<Vec<TimelineItem>> {
    fn on_update(&self, items: Vec<TimelineItem>) {
        self.send(items);
    }
}

impl ConnectionListener for Forward<ConnectionState> {
    fn on_update(&self, state: ConnectionState) {
        self.send(state);
    }
}

/// Wait for an update matching `pred`.
fn wait_for<T>(what: &str, updates: &mpsc::Receiver<T>, pred: impl Fn(&T) -> bool) -> T {
    let deadline = std::time::Instant::now() + TIMEOUT;
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        match updates.recv_timeout(left) {
            Ok(update) if pred(&update) => return update,
            Ok(_) => {}
            Err(e) => panic!("{what}: {e}"),
        }
    }
}

fn message_body(item: &TimelineItem) -> Option<&str> {
    match &item.content {
        TimelineItemContent::Event { event } => match &event.content {
            EventContent::Message { message } => Some(&message.body),
            _ => None,
        },
        _ => None,
    }
}

#[test]
#[ignore = "needs the local Synapse (testserver/run.sh)"]
fn sign_in_list_rooms_read_and_send_then_sign_out() {
    let core_runtime = tokio::runtime::Runtime::new().unwrap();
    let name = unique_name("ffi");
    core_runtime.block_on(register(&name)).unwrap();

    // Another device of the same user makes a room with an image in it.
    let other_dir = tempfile::tempdir().unwrap();
    let image = red_dot_png();
    let room_id = core_runtime.block_on(async {
        let profile = Profile::new("other", other_dir.path(), SecretStore::File).unwrap();
        let other = mtrx_core::Session::login(profile, &homeserver(), &name, PASSWORD)
            .await
            .unwrap();
        let room = other
            .create_room(NewRoom {
                name: Some("FFI".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        other.start_sync().await;
        let timeline = other.timeline(&room).await.unwrap();
        let attachment = Attachment {
            filename: "dot.png".into(),
            mimetype: "image/png".into(),
            bytes: image.clone(),
        };
        let sent = timeline.send("", vec![attachment], None).await.unwrap();
        tokio::time::timeout(TIMEOUT, sent.sent())
            .await
            .unwrap()
            .unwrap();
        other.stop_sync().await;
        room.to_string()
    });

    let (data, cache) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let secrets = Arc::new(Memory::default());
    let mtrx = Mtrx::new(
        data.path().to_string_lossy().into(),
        cache.path().to_string_lossy().into(),
        secrets.clone(),
    );
    assert!(block_on(mtrx.restore()).unwrap().is_none());
    let session = block_on(mtrx.login(homeserver(), name.clone(), PASSWORD.into())).unwrap();
    assert!(session.user_id().starts_with(&format!("@{name}:")));
    assert!(secrets.0.lock().unwrap().contains_key("profiles/default"));
    // Restoring while signed in opens the same account.
    drop(session);
    let session = block_on(mtrx.restore()).unwrap().expect("saved session");

    let (connection, connections) = forward();
    let _connection = session.subscribe_connection(connection);
    let (rooms, room_updates) = forward();
    let rooms_handle = block_on(session.subscribe_room_list(rooms)).unwrap();
    block_on(session.start_sync());
    wait_for("online", &connections, |c| *c == ConnectionState::Online);
    let state = wait_for("room list with the room", &room_updates, |s| {
        s.loaded && s.rooms.iter().any(|r| r.id == room_id)
    });
    assert_eq!(
        state.rooms.iter().find(|r| r.id == room_id).unwrap().name,
        "FFI"
    );
    rooms_handle.cancel();

    let timeline = block_on(session.timeline(room_id.clone())).unwrap();
    let (items, item_updates) = forward();
    let _timeline_handle = timeline.subscribe(items);
    block_on(timeline.paginate_back(20)).unwrap();
    let with_image = wait_for("the image", &item_updates, |items| {
        items.iter().any(|item| match &item.content {
            TimelineItemContent::Event { event } => matches!(
                &event.content,
                EventContent::Message { message } if message.media.is_some()
            ),
            _ => false,
        })
    });
    let media = with_image
        .iter()
        .find_map(|item| match &item.content {
            TimelineItemContent::Event { event } => match &event.content {
                EventContent::Message { message } => message.media.clone(),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    assert_eq!(media.kind, MediaKind::Image);
    assert_eq!(
        block_on(session.media_preview(media.clone())).unwrap(),
        image
    );
    let path = block_on(session.media_file(media.clone())).unwrap();
    assert!(path.starts_with(&*cache.path().to_string_lossy()));
    assert!(path.ends_with(".png"));
    assert_eq!(std::fs::read(&path).unwrap(), image);
    // A second time from the cache.
    assert_eq!(block_on(session.media_file(media)).unwrap(), path);

    block_on(timeline.send_text("hello **world**".into())).unwrap();
    let items = wait_for("our message, sent", &item_updates, |items| {
        items.iter().any(|item| {
            message_body(item) == Some("hello world")
                && matches!(&item.content, TimelineItemContent::Event { event }
                    if event.own && event.event_id.is_some())
        })
    });
    let ids: std::collections::HashSet<_> = items.iter().map(|i| &i.id).collect();
    assert_eq!(ids.len(), items.len(), "item IDs are unique");
    block_on(timeline.mark_read()).unwrap();

    block_on(session.logout()).unwrap();
    assert!(secrets.0.lock().unwrap().is_empty());
    assert!(!std::path::Path::new(&path).exists());
    assert!(block_on(mtrx.restore()).unwrap().is_none());
}

/// A 1×1 PNG, opaque red.
fn red_dot_png() -> Vec<u8> {
    vec![
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
        0x77, 0x53, 0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x08, 0xd7, 0x63, 0xf8,
        0xcf, 0xc0, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xdd, 0x8d, 0xb0, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ]
}
