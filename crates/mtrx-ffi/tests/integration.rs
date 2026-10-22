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
    ConnectionListener, ConnectionState, EncryptionState, EventContent, MediaKind, Message,
    MessageKind, Mtrx, MtrxError, QuoteState, Reaction, Reader, RecoveryListener, RecoveryStatus,
    RecoveryStep, ReplyQuote, RoomKind, RoomListListener, RoomListState, SecretBackend,
    SecretError, SendState, Session, TimelineEvent, TimelineItem, TimelineItemContent,
    TimelineListener, TypingListener, Verification, VerificationListener,
    VerificationRequestListener, VerificationState,
};
use tempfile::TempDir;

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

impl TypingListener for Forward<Vec<String>> {
    fn on_update(&self, names: Vec<String>) {
        self.send(names);
    }
}

impl ConnectionListener for Forward<ConnectionState> {
    fn on_update(&self, state: ConnectionState) {
        self.send(state);
    }
}

impl RecoveryListener for Forward<RecoveryStep> {
    fn on_update(&self, step: RecoveryStep) {
        self.send(step);
    }
}

impl VerificationListener for Forward<VerificationState> {
    fn on_update(&self, state: VerificationState) {
        self.send(state);
    }
}

impl VerificationRequestListener for Forward<Arc<Verification>> {
    fn on_request(&self, verification: Arc<Verification>) {
        self.send(verification);
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

/// A signed-in app with its own directories, syncing.
struct Device {
    session: Arc<Session>,
    _dirs: (TempDir, TempDir),
}

fn sign_in(name: &str) -> Device {
    let (data, cache) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let mtrx = Mtrx::new(
        data.path().to_string_lossy().into(),
        cache.path().to_string_lossy().into(),
        Arc::new(Memory::default()),
    );
    let session = block_on(mtrx.login(homeserver(), name.into(), PASSWORD.into())).unwrap();
    block_on(session.start_sync());
    Device {
        session,
        _dirs: (data, cache),
    }
}

/// Another device of user `name`, outside the bindings (which cannot make
/// rooms yet), makes an encrypted room and signs out.
fn encrypted_room(core_runtime: &tokio::runtime::Runtime, name: &str) -> String {
    core_runtime.block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let profile = Profile::new("other", dir.path(), SecretStore::File).unwrap();
        let other = mtrx_core::Session::login(profile, &homeserver(), name, PASSWORD)
            .await
            .unwrap();
        let room = other
            .create_room(NewRoom {
                name: Some("diary".into()),
                encrypted: true,
                ..Default::default()
            })
            .await
            .unwrap();
        other.logout().await.unwrap();
        room.to_string()
    })
}

/// Wait until the room list has `room_id`, known to be encrypted. A room
/// can show before its encryption event has synced (the server creates its
/// state one event at a time); what is sent until then goes out unencrypted.
fn wait_for_encrypted_room(session: &Session, room_id: &str) {
    let (rooms, updates) = forward();
    let _handle = block_on(session.subscribe_room_list(rooms)).unwrap();
    wait_for(
        "the encrypted room in the room list",
        &updates,
        |s: &RoomListState| s.loaded && s.rooms.iter().any(|r| r.id == room_id && r.encrypted),
    );
}

/// Send `text` to `room_id` and wait until the server has it.
fn send(session: &Session, room_id: &str, text: &str) {
    let timeline = block_on(session.timeline(room_id.into())).unwrap();
    let (items, updates) = forward();
    let _handle = timeline.subscribe(items);
    block_on(timeline.send_text(text.into())).unwrap();
    wait_for("sent", &updates, |items: &Vec<TimelineItem>| {
        items.iter().any(|item| {
            message_body(item) == Some(text)
                && matches!(&item.content, TimelineItemContent::Event { event }
                    if event.event_id.is_some())
        })
    });
}

fn is_undecryptable(item: &TimelineItem) -> bool {
    matches!(&item.content, TimelineItemContent::Event { event }
        if event.content == EventContent::UnableToDecrypt)
}

/// Load the whole room into `timeline`.
fn paginate_to_start(timeline: &mtrx_ffi::Timeline) {
    for _ in 0..10 {
        if block_on(timeline.paginate_back(20)).unwrap() {
            return;
        }
    }
    panic!("the room's start was not reached");
}

/// Poll `session`'s encryption state until `pred` holds.
fn wait_for_encryption(session: &Session, what: &str, pred: impl Fn(&EncryptionState) -> bool) {
    let deadline = std::time::Instant::now() + TIMEOUT;
    loop {
        let state = block_on(session.encryption_state()).unwrap();
        if pred(&state) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "{what}: still {state:?}"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[test]
#[ignore = "needs the local Synapse (testserver/run.sh)"]
fn recovery_key_decrypts_history_on_a_new_device() {
    let core_runtime = tokio::runtime::Runtime::new().unwrap();
    let name = unique_name("ffi-recovery");
    core_runtime.block_on(register(&name)).unwrap();

    // The first device sets up cross-signing; it writes in an encrypted
    // room, then turns on recovery.
    let first = sign_in(&name);
    let room_id = encrypted_room(&core_runtime, &name);
    wait_for_encrypted_room(&first.session, &room_id);
    send(&first.session, &room_id, "written before recovery");
    assert_eq!(
        block_on(first.session.encryption_state()).unwrap(),
        EncryptionState {
            device_verified: true,
            recovery: RecoveryStatus::Disabled,
        }
    );
    let key = block_on(first.session.enable_recovery()).unwrap();
    assert_eq!(first.session.recovery_status(), RecoveryStatus::Enabled);

    // A new device cannot read it, and says why.
    let second = sign_in(&name);
    wait_for_encrypted_room(&second.session, &room_id);
    wait_for_encryption(&second.session, "unverified, recovery incomplete", |s| {
        *s == EncryptionState {
            device_verified: false,
            recovery: RecoveryStatus::Incomplete,
        }
    });
    let timeline = block_on(second.session.timeline(room_id.clone())).unwrap();
    let (items, item_updates) = forward();
    let _timeline_handle = timeline.subscribe(items);
    paginate_to_start(&timeline);
    wait_for("an undecryptable message", &item_updates, |items| {
        items.iter().any(is_undecryptable)
    });

    // A mistyped key, or another backup's, fails while checking it.
    let another = matrix_sdk_crypto::secret_storage::SecretStorageKey::new().to_base58();
    for wrong in ["not a recovery key".to_owned(), another] {
        let (steps, step_updates) = forward();
        let refused = block_on(second.session.recover(wrong.clone(), steps));
        assert!(
            matches!(&refused, Err(MtrxError::InvalidInput { reason })
                if reason == "The recovery key is not valid"),
            "{wrong}: {refused:?}"
        );
        assert_eq!(
            step_updates.try_iter().collect::<Vec<_>>(),
            [RecoveryStep::CheckingKey]
        );
    }

    // The key unlocks it; the open timeline shows the message.
    let (steps, step_updates) = forward();
    block_on(second.session.recover(key, steps)).unwrap();
    let steps: Vec<_> = step_updates.try_iter().collect();
    assert_eq!(
        steps[..2],
        [RecoveryStep::CheckingKey, RecoveryStep::ImportingIdentity]
    );
    assert!(
        steps[2..]
            .iter()
            .all(|s| matches!(s, RecoveryStep::DownloadingKeys { .. })),
        "{steps:?}"
    );
    assert!(!steps[2..].is_empty(), "keys of the room were fetched");
    wait_for("the message, decrypted", &item_updates, |items| {
        items
            .iter()
            .any(|i| message_body(i) == Some("written before recovery"))
            && !items.iter().any(is_undecryptable)
    });
    wait_for_encryption(&second.session, "verified, recovery enabled", |s| {
        *s == EncryptionState {
            device_verified: true,
            recovery: RecoveryStatus::Enabled,
        }
    });
}

#[test]
#[ignore = "needs the local Synapse (testserver/run.sh)"]
fn emoji_verification_between_two_devices() {
    let core_runtime = tokio::runtime::Runtime::new().unwrap();
    let name = unique_name("ffi-verify");
    core_runtime.block_on(register(&name)).unwrap();
    let old = sign_in(&name);
    let room_id = encrypted_room(&core_runtime, &name);
    wait_for_encrypted_room(&old.session, &room_id);
    send(&old.session, &room_id, "written before verification");

    let new = sign_in(&name);
    assert!(block_on(old.session.is_device_verified()).unwrap());
    assert!(!block_on(new.session.is_device_verified()).unwrap());
    // The new device cannot read what was written before it existed.
    wait_for_encrypted_room(&new.session, &room_id);
    let timeline = block_on(new.session.timeline(room_id)).unwrap();
    let (items, item_updates) = forward();
    let _timeline_handle = timeline.subscribe(items);
    paginate_to_start(&timeline);
    wait_for("an undecryptable message", &item_updates, |items| {
        items.iter().any(is_undecryptable)
    });
    // A request from a device the other does not know yet is dropped.
    for (device, other) in [(&old, &new), (&new, &old)] {
        let other = other.session.device_id();
        let deadline = std::time::Instant::now() + TIMEOUT;
        while !block_on(device.session.devices())
            .unwrap()
            .iter()
            .any(|d| d.id == other && !d.current)
        {
            assert!(
                std::time::Instant::now() < deadline,
                "devices know each other"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    // The new device asks; the old one accepts.
    let (requests, incoming) = forward();
    let _requests_handle = old.session.subscribe_verification_requests(requests);
    let outgoing = block_on(new.session.request_self_verification()).unwrap();
    assert_eq!(outgoing.other_user(), new.session.user_id());
    let (listener, outgoing_states) = forward();
    let _outgoing_handle = outgoing.subscribe(listener);
    let accepted = incoming.recv_timeout(TIMEOUT).expect("incoming request");
    assert_eq!(accepted.other_user(), old.session.user_id());
    let (listener, accepted_states) = forward();
    let _accepted_handle = accepted.subscribe(listener);
    block_on(accepted.accept()).unwrap();

    // Both show the same emoji; both confirm.
    let emojis = |state: &VerificationState| matches!(state, VerificationState::Emojis { .. });
    let a = wait_for("emoji (new)", &outgoing_states, emojis);
    let b = wait_for("emoji (old)", &accepted_states, emojis);
    assert_eq!(a, b, "both devices show the same emoji");
    let VerificationState::Emojis { emojis } = &a else {
        unreachable!()
    };
    assert_eq!(emojis.len(), 7);
    assert!(
        emojis
            .iter()
            .all(|e| !e.symbol.is_empty() && !e.description.is_empty())
    );
    assert_eq!(outgoing.state(), a);

    block_on(outgoing.confirm()).unwrap();
    block_on(accepted.confirm()).unwrap();
    let done = |state: &VerificationState| *state == VerificationState::Done;
    wait_for("done (new)", &outgoing_states, done);
    wait_for("done (old)", &accepted_states, done);
    assert!(block_on(new.session.is_device_verified()).unwrap());

    // Now it can, in the timeline already open.
    wait_for("the message, decrypted", &item_updates, |items| {
        items
            .iter()
            .any(|i| message_body(i) == Some("written before verification"))
            && !items.iter().any(is_undecryptable)
    });
}

/// `ids` are in the order of `state.rooms`.
fn in_list_order(state: &RoomListState, ids: &[String]) -> bool {
    let positions: Vec<_> = ids
        .iter()
        .map(|id| state.rooms.iter().position(|r| &r.id == id).unwrap())
        .collect();
    positions.is_sorted()
}

#[test]
#[ignore = "needs the local Synapse (testserver/run.sh)"]
fn rooms_by_space_with_direct_chats_and_invites() {
    let core_runtime = tokio::runtime::Runtime::new().unwrap();
    let [alice, bob, carol] = ["ffi-spaces", "ffi-spaces-bob", "ffi-spaces-carol"].map(|prefix| {
        let name = unique_name(prefix);
        core_runtime.block_on(register(&name)).unwrap();
        name
    });

    // Alice's other device makes two spaces, rooms in one of them (one
    // through a nested space) and a room outside; Bob joins the space and
    // has a direct chat with her. Bob invites her to a room in the space;
    // Carol, in no space, invites her to a direct chat and a room.
    let dir = tempfile::tempdir().unwrap();
    let ids = core_runtime.block_on(async {
        let login = |name: &str, profile: &str| {
            let profile = Profile::new(profile, dir.path(), SecretStore::File).unwrap();
            let name = name.to_owned();
            async move {
                mtrx_core::Session::login(profile, &homeserver(), &name, PASSWORD)
                    .await
                    .unwrap()
            }
        };
        let (a, b, c) = (
            login(&alice, "alice").await,
            login(&bob, "bob").await,
            login(&carol, "carol").await,
        );
        let make = |name: &str, space: bool, parent: Option<&mtrx_core::OwnedRoomId>| NewRoom {
            name: Some(name.into()),
            space,
            parent: parent.cloned(),
            ..Default::default()
        };
        let team = a.create_room(make("Team", true, None)).await.unwrap();
        let other = a.create_room(make("Other", true, None)).await.unwrap();
        let lab = a
            .create_room(make("Lab", false, Some(&team)))
            .await
            .unwrap();
        let sub = a.create_room(make("Sub", true, Some(&team))).await.unwrap();
        let deep = a
            .create_room(make("Deep", false, Some(&sub)))
            .await
            .unwrap();
        let loose = a.create_room(make("Loose", false, None)).await.unwrap();
        a.invite(&team, &b.user_id()).await.unwrap();
        b.join(<&mtrx_core::RoomOrAliasId>::from(&*team))
            .await
            .unwrap();
        let with_bob = a.direct_chat(&b.user_id()).await.unwrap();
        b.join(<&mtrx_core::RoomOrAliasId>::from(&*with_bob))
            .await
            .unwrap();
        let offer = b
            .create_room(NewRoom {
                name: Some("Offer".into()),
                invite: vec![a.user_id()],
                ..Default::default()
            })
            .await
            .unwrap();
        a.add_to_space(&team, &offer).await.unwrap();
        let with_carol = c.direct_chat(&a.user_id()).await.unwrap();
        let party = c
            .create_room(NewRoom {
                name: Some("Party".into()),
                invite: vec![a.user_id()],
                ..Default::default()
            })
            .await
            .unwrap();
        [
            team, other, lab, sub, deep, loose, with_bob, offer, with_carol, party,
        ]
        .map(|id| id.to_string())
    });
    let [
        team,
        other,
        lab,
        sub,
        deep,
        loose,
        with_bob,
        offer,
        with_carol,
        party,
    ] = ids;

    let device = sign_in(&alice);
    let (rooms, updates) = forward();
    let _handle = block_on(device.session.subscribe_room_list(rooms)).unwrap();
    let state = wait_for("the rooms by space", &updates, |s: &RoomListState| {
        let team_rooms = s.spaces.iter().find(|space| space.space_id == team);
        s.loaded
            && team_rooms.is_some_and(|t| t.room_ids.contains(&with_bob))
            && [&loose, &party]
                .iter()
                .all(|id| s.rooms_outside_spaces.contains(id))
            && s.rooms.iter().any(|r| r.id == with_carol)
    });

    // The space column: the two top-level spaces, in list order.
    let spaces: Vec<_> = state.spaces.iter().map(|s| s.space_id.clone()).collect();
    assert_eq!(
        spaces.iter().collect::<std::collections::HashSet<_>>(),
        [&team, &other]
            .into_iter()
            .collect::<std::collections::HashSet<_>>()
    );
    assert!(in_list_order(&state, &spaces));

    // Team lists its rooms through the nested space (not the nested space
    // itself), the invite, and the direct chat with its member Bob.
    let team_rooms = &state
        .spaces
        .iter()
        .find(|s| s.space_id == team)
        .unwrap()
        .room_ids;
    let mut expected = vec![lab, deep, offer.clone(), with_bob.clone()];
    expected.sort();
    let mut listed = team_rooms.clone();
    listed.sort();
    assert_eq!(listed, expected, "{state:#?}");
    assert!(in_list_order(&state, team_rooms));
    assert!(!team_rooms.contains(&sub));
    let other_rooms = &state
        .spaces
        .iter()
        .find(|s| s.space_id == other)
        .unwrap()
        .room_ids;
    assert!(other_rooms.is_empty());

    // Outside any space: the room and the invite to a room, in list order.
    assert_eq!(state.rooms_outside_spaces.len(), 2, "{state:#?}");
    assert!(in_list_order(&state, &state.rooms_outside_spaces));

    // The Chats tab: both direct chats; Carol's still an invite.
    let summary = |id: &str| state.rooms.iter().find(|r| r.id == id).unwrap();
    assert_eq!(summary(&with_bob).kind, RoomKind::Direct);
    assert_eq!(
        (summary(&with_carol).kind, summary(&with_carol).invited),
        (RoomKind::Direct, true)
    );
    assert!(summary(&offer).invited);
    assert_eq!(summary(&sub).kind, RoomKind::Space);
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

/// A timeline's items as its listener gets them, and the latest.
struct Items {
    updates: mpsc::Receiver<Vec<TimelineItem>>,
    latest: Vec<TimelineItem>,
    _handle: Arc<mtrx_ffi::TaskHandle>,
}

impl Items {
    fn follow(timeline: &mtrx_ffi::Timeline) -> Self {
        let (items, updates) = forward();
        Self {
            _handle: timeline.subscribe(items),
            updates,
            latest: Vec::new(),
        }
    }

    /// Wait until the items match `pred` (maybe they do already).
    fn wait(&mut self, what: &str, pred: impl Fn(&[TimelineItem]) -> bool) -> &[TimelineItem] {
        if !pred(&self.latest) {
            self.latest = wait_for(what, &self.updates, |items| pred(items));
        }
        &self.latest
    }
}

/// The row (its ID) and event of the message whose text is `body`.
fn find<'a>(items: &'a [TimelineItem], body: &str) -> Option<(&'a str, &'a TimelineEvent)> {
    items.iter().find_map(|item| match &item.content {
        TimelineItemContent::Event { event } if message_body(item) == Some(body) => {
            Some((item.id.as_str(), event))
        }
        _ => None,
    })
}

fn message(event: &TimelineEvent) -> &Message {
    match &event.content {
        EventContent::Message { message } => message,
        other => panic!("not a message: {other:?}"),
    }
}

/// The message with `body`, sent (with an event ID).
fn sent<'a>(items: &'a [TimelineItem], body: &str) -> Option<(&'a str, &'a TimelineEvent)> {
    find(items, body).filter(|(_, event)| event.event_id.is_some())
}

/// Wait until the room list has `room_id`.
fn wait_for_room(session: &Session, room_id: &str) {
    let (rooms, updates) = forward();
    let _handle = block_on(session.subscribe_room_list(rooms)).unwrap();
    wait_for(
        "the room in the room list",
        &updates,
        |s: &RoomListState| s.rooms.iter().any(|r| r.id == room_id && !r.invited),
    );
}

/// Users `names` made, and a room made by the first, which the others
/// joined, all outside the bindings (which cannot make rooms yet).
fn room_of(core_runtime: &tokio::runtime::Runtime, prefixes: &[&str]) -> (Vec<String>, String) {
    let names: Vec<String> = prefixes
        .iter()
        .map(|prefix| {
            let name = unique_name(prefix);
            core_runtime.block_on(register(&name)).unwrap();
            name
        })
        .collect();
    let dir = tempfile::tempdir().unwrap();
    let room = core_runtime.block_on(async {
        let mut sessions = Vec::new();
        for name in &names {
            let profile = Profile::new(name, dir.path(), SecretStore::File).unwrap();
            sessions.push(
                mtrx_core::Session::login(profile, &homeserver(), name, PASSWORD)
                    .await
                    .unwrap(),
            );
        }
        let room = sessions[0]
            .create_room(NewRoom {
                name: Some("talk".into()),
                invite: sessions[1..].iter().map(|s| s.user_id()).collect(),
                ..Default::default()
            })
            .await
            .unwrap();
        for other in &sessions[1..] {
            other
                .join(<&mtrx_core::RoomOrAliasId>::from(&*room))
                .await
                .unwrap();
        }
        for session in sessions {
            session.logout().await.unwrap();
        }
        room.to_string()
    });
    (names, room)
}

#[test]
#[ignore = "needs the local Synapse (testserver/run.sh)"]
fn reactions_replies_edits_receipts_and_typing() {
    let core_runtime = tokio::runtime::Runtime::new().unwrap();
    let (names, room_id) = room_of(&core_runtime, &["ffi-talk", "ffi-talk-bob"]);
    let (alice, bob) = (sign_in(&names[0]), sign_in(&names[1]));
    let (alice_id, bob_id) = (alice.session.user_id(), bob.session.user_id());
    let reader = |id: &str, name: &str| Reader {
        user_id: id.into(),
        name: Some(name.into()),
        avatar_url: None,
    };
    let (alice_reader, bob_reader) = (reader(&alice_id, &names[0]), reader(&bob_id, &names[1]));
    wait_for_room(&alice.session, &room_id);
    wait_for_room(&bob.session, &room_id);
    let alice_timeline = block_on(alice.session.timeline(room_id.clone())).unwrap();
    let bob_timeline = block_on(bob.session.timeline(room_id.clone())).unwrap();
    let mut alice_items = Items::follow(&alice_timeline);
    let mut bob_items = Items::follow(&bob_timeline);

    // Bob asks; Alice replies, quoting him.
    block_on(bob_timeline.send_text("Which dataset?".into())).unwrap();
    let items = alice_items.wait("Alice gets the question", |i| {
        sent(i, "Which dataset?").is_some()
    });
    let (question_row, question) = sent(items, "Which dataset?").unwrap();
    let (question_row, question_id) = (question_row.to_owned(), question.event_id.clone());
    assert!(!question.editable);
    block_on(alice_timeline.reply(question_row.clone(), "The **new** one".into())).unwrap();
    let items = bob_items.wait("Bob sees the reply with its quote", |i| {
        find(i, "The new one").is_some_and(|(_, e)| {
            message(e)
                .reply_to
                .as_ref()
                .is_some_and(|q| q.state == QuoteState::Loaded && q.sender_name.is_some())
        })
    });
    let reply = find(items, "The new one").unwrap().1;
    assert_eq!(
        message(reply).reply_to,
        Some(ReplyQuote {
            event_id: question_id.unwrap(),
            state: QuoteState::Loaded,
            sender_id: Some(bob_id.clone()),
            sender_name: Some(names[1].clone()),
            kind: Some(MessageKind::Text),
            body: Some("Which dataset?".into()),
        })
    );
    let bob_question_row = find(items, "Which dataset?").unwrap().0.to_owned();

    // Reactions, counted and named on both sides, toggled on and off.
    let reactions = |items: &[TimelineItem]| {
        find(items, "Which dataset?").map(|(_, e)| message(e).reactions.clone())
    };
    let thumbs = |count, by_us, senders: &[&Reader]| {
        Some(vec![Reaction {
            key: "👍".into(),
            count,
            by_us,
            senders: senders.iter().map(|&r| r.clone()).collect(),
        }])
    };
    block_on(alice_timeline.react(question_row.clone(), "👍".into())).unwrap();
    bob_items.wait("Bob sees Alice's reaction", |i| {
        reactions(i) == thumbs(1, false, &[&alice_reader])
    });
    block_on(bob_timeline.react(bob_question_row.clone(), "👍".into())).unwrap();
    alice_items.wait("Alice sees both reactions", |i| {
        reactions(i) == thumbs(2, true, &[&alice_reader, &bob_reader])
    });
    bob_items.wait("Bob sees both reactions", |i| {
        reactions(i) == thumbs(2, true, &[&alice_reader, &bob_reader])
    });
    block_on(alice_timeline.react(question_row, "👍".into())).unwrap();
    alice_items.wait("Alice sees only Bob's", |i| {
        reactions(i) == thumbs(1, false, &[&bob_reader])
    });
    bob_items.wait("Bob sees only his", |i| {
        reactions(i) == thumbs(1, true, &[&bob_reader])
    });
    block_on(bob_timeline.react(bob_question_row, "👍".into())).unwrap();
    alice_items.wait("no reactions left", |i| reactions(i) == Some(Vec::new()));

    // Alice edits her reply, then deletes it.
    let items = alice_items.wait("Alice's reply sent", |i| sent(i, "The new one").is_some());
    let (reply_row, reply) = sent(items, "The new one").unwrap();
    assert!(reply.editable);
    assert_eq!(message(reply).raw_body, "The **new** one");
    let reply_row = reply_row.to_owned();
    block_on(alice_timeline.edit(reply_row.clone(), "The *newest* one".into())).unwrap();
    let items = bob_items.wait("Bob sees the edit", |i| {
        find(i, "The newest one").is_some_and(|(_, e)| message(e).edited)
    });
    let bob_reply_row = find(items, "The newest one").unwrap().0.to_owned();
    let items = alice_items.wait("Alice sees the edit", |i| {
        find(i, "The newest one").is_some_and(|(_, e)| message(e).edited)
    });
    assert_eq!(
        message(find(items, "The newest one").unwrap().1).raw_body,
        "The *newest* one"
    );
    block_on(alice_timeline.delete(reply_row)).unwrap();
    bob_items.wait("Bob sees it deleted", |i| {
        i.iter().any(|item| {
            item.id == bob_reply_row
                && matches!(&item.content, TimelineItemContent::Event { event }
                    if event.content == EventContent::Redacted)
        })
    });

    // Typing, by name.
    let (typing, names_typing) = forward();
    let _typing = bob_timeline.subscribe_typing(typing);
    block_on(alice_timeline.set_typing(true)).unwrap();
    wait_for("Bob sees Alice typing", &names_typing, |n: &Vec<String>| {
        *n == [names[0].clone()]
    });
    block_on(alice_timeline.set_typing(false)).unwrap();
    wait_for("Bob sees Alice stop", &names_typing, |n: &Vec<String>| {
        n.is_empty()
    });

    // Bob reads Alice's latest message: his receipt moves there, with his
    // name.
    block_on(alice_timeline.send_text("Thanks".into())).unwrap();
    bob_items.wait("Bob gets the thanks", |i| sent(i, "Thanks").is_some());
    let read_by_bob = |items: &[TimelineItem]| -> Vec<String> {
        items
            .iter()
            .filter(|item| {
                matches!(&item.content, TimelineItemContent::Event { event }
                if event.read_by.contains(&bob_reader))
            })
            .map(|item| item.id.clone())
            .collect()
    };
    let items = alice_items.wait("the thanks sent", |i| sent(i, "Thanks").is_some());
    let thanks_row = sent(items, "Thanks").unwrap().0.to_owned();
    assert!(!read_by_bob(items).contains(&thanks_row));
    block_on(bob_timeline.mark_read()).unwrap();
    alice_items.wait("Alice sees Bob's receipt", |i| {
        read_by_bob(i) == [thanks_row.clone()]
    });
}

#[test]
#[ignore = "needs the local Synapse (testserver/run.sh)"]
fn failed_sends_are_retried_or_discarded() {
    let core_runtime = tokio::runtime::Runtime::new().unwrap();
    let (names, room_id) = room_of(&core_runtime, &["ffi-unsent"]);
    let device = sign_in(&names[0]);
    wait_for_room(&device.session, &room_id);
    let timeline = block_on(device.session.timeline(room_id)).unwrap();
    let mut items = Items::follow(&timeline);

    // The server refuses events over 64 KiB, whatever we try.
    let too_long = "x".repeat(70_000);
    let failed = |i: &[TimelineItem]| {
        find(i, &too_long).is_some_and(|(_, e)| matches!(e.send_state, SendState::Failed { .. }))
    };
    block_on(timeline.send_text(too_long.clone())).unwrap();
    let row = find(items.wait("shown as failed", failed), &too_long)
        .unwrap()
        .0
        .to_owned();

    // Sent again: sending, then failed again.
    block_on(timeline.retry(row.clone())).unwrap();
    items.wait("sending again", |i| {
        find(i, &too_long).is_some_and(|(_, e)| e.send_state == SendState::Sending)
    });
    items.wait("failed again", failed);

    // What is queued after it waits for it; once it is dropped, it is sent.
    block_on(timeline.send_text("after the long one".into())).unwrap();
    block_on(timeline.discard(row)).unwrap();
    let items = items.wait("dropped, and the next one sent", |i| {
        find(i, &too_long).is_none()
            && sent(i, "after the long one").is_some_and(|(_, e)| e.send_state == SendState::Sent)
    });
    let row = sent(items, "after the long one").unwrap().0.to_owned();
    assert!(matches!(
        block_on(timeline.retry(row.clone())),
        Err(MtrxError::Other { .. })
    ));
    assert!(matches!(
        block_on(timeline.discard(row)),
        Err(MtrxError::Other { .. })
    ));
    assert!(matches!(
        block_on(timeline.react("no-such-row".into(), "👍".into())),
        Err(MtrxError::InvalidInput { .. })
    ));
}
