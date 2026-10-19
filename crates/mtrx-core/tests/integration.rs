//! End-to-end tests against the local Synapse from `testserver/`.
//!
//! ```sh
//! testserver/run.sh
//! cargo test -p mtrx-core --test integration -- --ignored
//! ```
//!
//! Each test registers fresh users, so tests are independent and can run in
//! parallel against a long-lived server.

#![recursion_limit = "256"]

use std::{
    future::Future,
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result, bail};
use futures_util::{Stream, StreamExt};
use mtrx_core::{
    CallService, EventContent, MembershipChange, NewRoom, Permissions, Profile, RecoveryStatus,
    RoomKind, SecretStore, Session, TimelineEntry, VerificationState,
    test_support::{self, PASSWORD, homeserver, unique_name},
};
use mtrx_format::{Block, Inline, Style};
use tempfile::TempDir;

const TIMEOUT: Duration = Duration::from_secs(30);

async fn register(name: &str) -> Result<()> {
    test_support::register(name)
        .await
        .map_err(anyhow::Error::msg)
}

/// A logged-in device with its own data directory.
struct Device {
    session: Session,
    _dir: TempDir,
}

async fn login(user: &str) -> Result<Device> {
    let dir = TempDir::new()?;
    let profile = Profile::new("test", Some(dir.path()), SecretStore::File)?;
    let session = Session::login(profile, &homeserver(), user, PASSWORD).await?;
    session.start_sync().await;
    Ok(Device { session, _dir: dir })
}

async fn new_user(prefix: &str) -> Result<(String, Device)> {
    let name = unique_name(prefix);
    register(&name).await?;
    let device = login(&name).await?;
    Ok((name, device))
}

async fn timeout<T>(what: &str, future: impl Future<Output = T>) -> Result<T> {
    tokio::time::timeout(TIMEOUT, future)
        .await
        .with_context(|| format!("timed out: {what}"))
}

/// Wait until a stream yields an item matching `pred`.
async fn wait_for<T>(
    what: &str,
    stream: impl Stream<Item = T>,
    mut pred: impl FnMut(&T) -> bool,
) -> Result<T> {
    let mut stream = std::pin::pin!(stream);
    timeout(what, async {
        while let Some(item) = stream.next().await {
            if pred(&item) {
                return Ok(item);
            }
        }
        bail!("stream ended: {what}")
    })
    .await?
}

fn has_message(entries: &[TimelineEntry], body: &str) -> bool {
    entries.iter().any(|e| match e {
        TimelineEntry::Event(ev) => {
            matches!(&ev.content, EventContent::Message(m) if m.body == body)
        }
        _ => false,
    })
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs local Synapse (testserver/run.sh)"]
async fn login_restore_logout() -> Result<()> {
    let name = unique_name("login");
    register(&name).await?;
    let dir = TempDir::new()?;
    let profile = || Profile::new("p", Some(dir.path()), SecretStore::File);

    let session = Session::login(profile()?, &homeserver(), &name, PASSWORD).await?;
    let device = session.device_id();
    drop(session);

    let session = Session::restore(profile()?).await?;
    assert_eq!(session.device_id(), device);
    assert_eq!(session.user_id().localpart(), name);
    session.logout().await?;

    assert!(matches!(
        Session::restore(profile()?).await,
        Err(mtrx_core::Error::NotLoggedIn(_))
    ));
    assert!(!dir.path().join("profiles/p").exists());

    let wrong = Session::login(profile()?, &homeserver(), &name, "wrong").await;
    assert!(wrong.is_err());
    assert!(
        !dir.path().join("profiles/p").exists(),
        "failed login leaves no data"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs local Synapse (testserver/run.sh)"]
async fn room_list_shows_space_hierarchy() -> Result<()> {
    let (_, alice) = new_user("spaces").await?;
    let s = &alice.session;
    let space = s
        .create_room(NewRoom {
            name: Some("Work".into()),
            space: true,
            ..Default::default()
        })
        .await?;
    let room = s
        .create_room(NewRoom {
            name: Some("general".into()),
            encrypted: true,
            ..Default::default()
        })
        .await?;
    s.add_to_space(&space, &room).await?;

    let list = s.room_list().await?;
    let state = wait_for("room in space", list.updates(), |state| {
        state
            .rooms
            .iter()
            .any(|r| r.id == room && r.parents.contains(&space))
    })
    .await?;
    let space_entry = state
        .rooms
        .iter()
        .find(|r| r.id == space)
        .context("space listed")?;
    assert_eq!(space_entry.kind, RoomKind::Space);
    assert_eq!(space_entry.name, "Work");
    let room_entry = state.rooms.iter().find(|r| r.id == room).unwrap();
    assert!(room_entry.encrypted);
    assert_eq!(s.find_room("general").await?, room);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs local Synapse (testserver/run.sh)"]
async fn encrypted_messages_between_users() -> Result<()> {
    let (_, alice) = new_user("alice").await?;
    let (_, bob) = new_user("bob").await?;
    let room = alice
        .session
        .create_room(NewRoom {
            name: Some("secret".into()),
            encrypted: true,
            invite: vec![bob.session.user_id()],
            ..Default::default()
        })
        .await?;

    let bob_list = bob.session.room_list().await?;
    wait_for("bob invited", bob_list.updates(), |s| {
        s.rooms.iter().any(|r| r.id == room && r.invited)
    })
    .await?;
    bob.session
        .join(<&mtrx_core::RoomOrAliasId>::from(&*room))
        .await?;

    // Alice must see bob joined, otherwise the room key is not shared with him.
    let alice_timeline = alice.session.timeline(&room).await?;
    wait_for("bob joined", alice_timeline.updates(), |entries| {
        entries.iter().any(|e| {
            matches!(e, TimelineEntry::Event(ev) if ev.sender == bob.session.user_id()
            && matches!(ev.content, EventContent::Membership {
                change: MembershipChange::InvitationAccepted, ..
            }))
        })
    })
    .await?;

    let event_id = timeout(
        "send",
        alice_timeline
            .send_markdown("hello **bob**", None)
            .await?
            .sent(),
    )
    .await??;

    let bob_timeline = bob.session.timeline(&room).await?;
    let entries = wait_for("bob decrypts", bob_timeline.updates(), |e| {
        has_message(e, "hello **bob**")
    })
    .await?;
    let message = entries
        .iter()
        .find_map(|e| match e {
            TimelineEntry::Event(ev) if ev.event_id.as_ref() == Some(&event_id) => Some(ev.clone()),
            _ => None,
        })
        .context("event by id")?;
    let EventContent::Message(m) = message.content else {
        bail!("not a message")
    };
    assert_eq!(
        m.document.blocks,
        vec![Block::Paragraph(vec![
            Inline::Text("hello ".into()),
            Inline::Styled {
                style: Style::Bold,
                content: vec![Inline::Text("bob".into())]
            },
        ])]
    );
    assert_eq!(message.sender, alice.session.user_id());
    Ok(())
}

/// A `size`×`size` PNG with a colour gradient.
fn test_png(size: u32) -> Vec<u8> {
    let img = image::RgbImage::from_fn(size, size, |x, y| {
        image::Rgb([(x * 255 / size) as u8, (y * 255 / size) as u8, 160])
    });
    let mut png = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    png
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs local Synapse (testserver/run.sh)"]
async fn avatars_in_room_list_and_timeline() -> Result<()> {
    let (_, alice) = new_user("alice").await?;
    let (_, bob) = new_user("bob").await?;
    let avatar = bob.session.set_avatar("image/png", test_png(128)).await?;

    let room = alice
        .session
        .create_room(NewRoom {
            invite: vec![bob.session.user_id()],
            direct: true,
            ..Default::default()
        })
        .await?;
    let bob_list = bob.session.room_list().await?;
    wait_for("bob invited", bob_list.updates(), |s| {
        s.rooms.iter().any(|r| r.id == room && r.invited)
    })
    .await?;
    bob.session
        .join(<&mtrx_core::RoomOrAliasId>::from(&*room))
        .await?;
    let bob_timeline = bob.session.timeline(&room).await?;
    timeout("send", bob_timeline.send_markdown("hi", None).await?.sent()).await??;

    // A DM without a room picture shows the other user's.
    let list = alice.session.room_list().await?;
    wait_for("DM with bob's avatar", list.updates(), |s| {
        s.rooms.iter().any(|r| {
            r.id == room && r.kind == RoomKind::Direct && r.avatar.as_ref() == Some(&avatar)
        })
    })
    .await?;
    let timeline = alice.session.timeline(&room).await?;
    wait_for(
        "message with sender avatar",
        timeline.updates(),
        |entries| {
            entries.iter().any(|e| {
                matches!(e, TimelineEntry::Event(ev)
                if ev.sender == bob.session.user_id() && ev.sender_avatar.as_ref() == Some(&avatar))
            })
        },
    )
    .await?;

    let thumbnail = alice.session.thumbnail(&avatar, 32).await?;
    let thumbnail = image::load_from_memory(&thumbnail)?;
    assert!(thumbnail.width() >= 32 && thumbnail.width() <= 128);
    assert_eq!(thumbnail.width(), thumbnail.height());
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs local Synapse (testserver/run.sh)"]
async fn recovery_decrypts_history_on_new_device() -> Result<()> {
    let (name, first) = new_user("recovery").await?;
    let room = first
        .session
        .create_room(NewRoom {
            name: Some("diary".into()),
            encrypted: true,
            ..Default::default()
        })
        .await?;
    let timeline = first.session.timeline(&room).await?;
    timeout(
        "send",
        timeline
            .send_markdown("written before recovery", None)
            .await?
            .sent(),
    )
    .await??;
    let key = first.session.enable_recovery().await?;
    assert_eq!(first.session.recovery_status(), RecoveryStatus::Enabled);

    let second = login(&name).await?;
    assert!(!second.session.is_device_verified().await?);
    second.session.room_list().await?.wait_loaded().await;
    second.session.recover(&key, |_| {}).await?;
    assert!(second.session.is_device_verified().await?);

    let timeline = second.session.timeline(&room).await?;
    // Room keys come from the backup when decryption first fails.
    let load = async {
        while !timeline.paginate_back(20).await? {}
        anyhow::Ok(())
    };
    timeout("paginate", load).await??;
    wait_for("decrypt from backup", timeline.updates(), |e| {
        has_message(e, "written before recovery")
    })
    .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs local Synapse (testserver/run.sh)"]
async fn emoji_self_verification() -> Result<()> {
    let (name, old) = new_user("verify").await?;
    let new = login(&name).await?;
    assert!(old.session.is_device_verified().await?);
    assert!(!new.session.is_device_verified().await?);
    // Both devices must know each other before verifying.
    old.session.room_list().await?.wait_loaded().await;
    new.session.room_list().await?.wait_loaded().await;

    let mut incoming = std::pin::pin!(old.session.verification_requests());
    let outgoing = new.session.request_self_verification().await?;
    let accepted = timeout("incoming request", incoming.next())
        .await?
        .context("no request")?;
    accepted.accept().await?;

    let emojis = |state: &VerificationState| matches!(state, VerificationState::Emojis(_));
    let a = wait_for("emojis (new)", outgoing.states(), emojis).await?;
    let b = wait_for("emojis (old)", accepted.states(), emojis).await?;
    assert_eq!(a, b, "both devices show the same emojis");

    outgoing.confirm().await?;
    accepted.confirm().await?;
    let done = |state: &VerificationState| matches!(state, VerificationState::Done);
    wait_for("done (new)", outgoing.states(), done).await?;
    wait_for("done (old)", accepted.states(), done).await?;
    assert!(new.session.is_device_verified().await?);
    Ok(())
}

/// Talks to matrix.org: registers the client with its OAuth provider and
/// checks we get a sign-in page URL, then cancels.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs network access to matrix.org"]
async fn matrix_org_offers_browser_login() -> Result<()> {
    assert!(Session::supports_oauth("matrix.org").await?);
    let dir = TempDir::new()?;
    let profile = Profile::new("oauth", Some(dir.path()), SecretStore::File)?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    let login = Session::login_oauth(profile, "matrix.org", move |url| {
        let _ = tx.send(url);
    });
    // The login waits for the browser forever; stop once the URL is known.
    let url = tokio::select! {
        url = rx => url?,
        result = login => bail!("login ended early: {:?}", result.err()),
    };
    println!("{url}");
    assert_eq!(url.scheme(), "https");
    assert!(
        url.query_pairs()
            .any(|(k, v)| k == "redirect_uri" && v.starts_with("http://127.0.0.1:"))
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs local Synapse (testserver/run.sh)"]
async fn scheduled_messages() -> Result<()> {
    let name = unique_name("later");
    register(&name).await?;
    let dir = TempDir::new()?;
    let profile = || Profile::new("p", Some(dir.path()), SecretStore::File);
    let session = Session::login(profile()?, &homeserver(), &name, PASSWORD).await?;
    session.start_sync().await;
    let room = session
        .create_room(NewRoom {
            name: Some("later".into()),
            encrypted: true,
            ..Default::default()
        })
        .await?;
    wait_for("room synced", session.room_list().await?.updates(), |s| {
        s.rooms.iter().any(|r| r.id == room)
    })
    .await?;
    let in_secs = |s| SystemTime::now() + Duration::from_secs(s);

    assert!(matches!(
        session
            .schedule(&room, "past", SystemTime::now(), None)
            .await,
        Err(mtrx_core::Error::SendTimeInPast)
    ));
    let late = session.schedule(&room, "late", in_secs(1), None).await?;
    let cancelled = session
        .schedule(&room, "cancelled", in_secs(3600), None)
        .await?;
    session.unschedule(&cancelled.id).await?;
    session.stop_sync().await;
    drop(session);

    // "late" passes its time while no scheduler runs.
    tokio::time::sleep(Duration::from_secs(3)).await;
    let session = Session::restore(profile()?).await?;
    assert_eq!(session.scheduled(), vec![late.clone()], "persisted");
    session.start_sync().await;
    let on_time = session
        .schedule(&room, "on **time**", in_secs(2), None)
        .await?;
    session.start_scheduler_for_test(Duration::from_secs(1));

    let timeline = session.timeline(&room).await?;
    wait_for("sent when due", timeline.updates(), |e| {
        has_message(e, "on **time**")
    })
    .await?;
    assert!(SystemTime::now() >= on_time.send_at);
    let held = wait_for("late one held", session.scheduled_updates(), |s| {
        s.iter().all(|m| m.overdue)
    })
    .await?;
    assert_eq!(held.len(), 1);
    assert_eq!(held[0].id, late.id);
    assert!(!has_message(&timeline.current(), "late"));

    let pending = session.send_scheduled_now(&late.id).await?;
    timeout("overdue one sent", pending.sent()).await??;
    assert!(session.scheduled().is_empty());
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs local Synapse (testserver/run.sh)"]
async fn call_links_and_space_services() -> Result<()> {
    let (_, alice) = new_user("calls").await?;
    let (_, bob) = new_user("calls").await?;
    let s = &alice.session;
    let space = s
        .create_room(NewRoom {
            name: Some("Lab".into()),
            space: true,
            ..Default::default()
        })
        .await?;
    let room = s
        .create_room(NewRoom {
            name: Some("meetings".into()),
            invite: vec![bob.session.user_id()],
            permissions: Permissions::Moderators,
            ..Default::default()
        })
        .await?;
    // Names come with the rooms' state, which includes the power levels.
    wait_for("rooms synced", s.room_list().await?.updates(), |state| {
        [(&space, "Lab"), (&room, "meetings")]
            .iter()
            .all(|(id, name)| state.rooms.iter().any(|r| &r.id == *id && r.name == *name))
    })
    .await?;

    let mut links = std::pin::pin!(s.call_link_updates(&room).await?);
    assert_eq!(timeout("initial", links.next()).await?, Some(None));
    let url = mtrx_core::Url::parse("https://meet.jit.si/mtrx-test")?;
    assert!(s.can_set_call_link(&room).await?);
    s.set_call_link(&room, Some(&url)).await?;
    assert_eq!(
        timeout("update", links.next()).await?,
        Some(Some(url.clone()))
    );
    assert_eq!(s.call_link(&room).await?, Some(url.clone()));
    let timeline = s.timeline(&room).await?;
    wait_for("link posted", timeline.updates(), |e| {
        has_message(e, &format!("Call link for this room: {url}"))
    })
    .await?;
    s.set_call_link(&room, None).await?;
    assert_eq!(timeout("removed", links.next()).await?, Some(None));

    let service = CallService::jitsi("Lab Jitsi", "https://video.lab.example")?;
    s.set_space_call_service(&space, Some(service.clone()))
        .await?;
    assert_eq!(s.space_call_service(&space).await?, Some(service));

    // In a moderated room, members cannot change room state.
    let bob_list = bob.session.room_list().await?;
    wait_for("bob invited", bob_list.updates(), |s| {
        s.rooms.iter().any(|r| r.id == room)
    })
    .await?;
    bob.session
        .join(<&mtrx_core::RoomOrAliasId>::from(&*room))
        .await?;
    wait_for("bob joined", bob_list.updates(), |s| {
        s.rooms
            .iter()
            .any(|r| r.id == room && !r.invited && r.name == "meetings")
    })
    .await?;
    // Power levels arrive with the joined room's state, after the join.
    let can_set = timeout("power levels synced", async {
        loop {
            match bob.session.can_set_call_link(&room).await {
                Ok(can_set) => return can_set,
                Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
            }
        }
    })
    .await?;
    assert!(!can_set);
    Ok(())
}

/// Join `room` as `user` and wait until its state (with the power levels) synced.
async fn join_synced(user: &Device, room: &mtrx_core::OwnedRoomId, name: &str) -> Result<()> {
    let list = user.session.room_list().await?;
    wait_for("invited", list.updates(), |s| {
        s.rooms.iter().any(|r| &r.id == room)
    })
    .await?;
    user.session
        .join(<&mtrx_core::RoomOrAliasId>::from(&**room))
        .await?;
    wait_for("joined", list.updates(), |s| {
        s.rooms
            .iter()
            .any(|r| &r.id == room && !r.invited && r.name == name)
    })
    .await?;
    timeout("power levels synced", async {
        while user.session.permissions(room).await.is_err() {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs local Synapse (testserver/run.sh)"]
async fn everyone_can_edit() -> Result<()> {
    let (_, alice) = new_user("perm").await?;
    let (_, bob) = new_user("perm").await?;
    let (_, carol) = new_user("perm").await?;
    let s = &alice.session;
    let new_room = |name: &str, space, permissions| NewRoom {
        name: Some(name.into()),
        space,
        invite: vec![bob.session.user_id()],
        permissions,
        ..Default::default()
    };

    // New rooms are open: bob may rename and invite, but not change permissions.
    let open = s
        .create_room(new_room("open", false, Permissions::Everyone))
        .await?;
    join_synced(&bob, &open, "open").await?;
    assert_eq!(bob.session.permissions(&open).await?, Permissions::Everyone);
    assert!(bob.session.can_set_call_link(&open).await?);
    assert!(!bob.session.can_change_permissions(&open).await?);
    bob.session.invite(&open, &carol.session.user_id()).await?;

    // A moderated space with a moderated room, opened by alice in one go.
    let space = s
        .create_room(new_room("team", true, Permissions::Moderators))
        .await?;
    let room = s
        .create_room(new_room("closed", false, Permissions::Moderators))
        .await?;
    s.add_to_space(&space, &room).await?;
    join_synced(&bob, &room, "closed").await?;
    assert_eq!(
        bob.session.permissions(&room).await?,
        Permissions::Moderators
    );
    assert!(!bob.session.can_set_call_link(&room).await?);
    wait_for("space synced", s.room_list().await?.updates(), |state| {
        state
            .rooms
            .iter()
            .any(|r| r.id == room && r.parents.contains(&space))
    })
    .await?;

    let update = s.let_everyone_edit_space(&space).await?;
    let mut changed = update.changed.clone();
    changed.sort();
    let mut expected = vec![space.clone(), room.clone()];
    expected.sort();
    assert_eq!(changed, expected);
    assert!(update.not_allowed.is_empty());

    timeout("bob sees the change", async {
        while bob.session.permissions(&room).await.ok() != Some(Permissions::Everyone) {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await?;
    assert!(bob.session.can_set_call_link(&room).await?);
    assert!(!bob.session.can_change_permissions(&room).await?);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs local Synapse (testserver/run.sh)"]
async fn room_management() -> Result<()> {
    let (_, alice) = new_user("manage").await?;
    let (bob_name, bob) = new_user("manage").await?;
    let s = &alice.session;
    let space = s
        .create_room(NewRoom {
            name: Some("Lab".into()),
            space: true,
            ..Default::default()
        })
        .await?;
    let room = s
        .create_room(NewRoom {
            name: Some("papers".into()),
            parent: Some(space.clone()),
            ..Default::default()
        })
        .await?;
    let list = s.room_list().await?;
    wait_for("room in space", list.updates(), |state| {
        state
            .rooms
            .iter()
            .any(|r| r.id == room && r.parents.contains(&space))
    })
    .await?;

    s.set_room_name(&room, "reading list").await?;
    s.set_room_topic(&room, "Papers to read").await?;
    wait_for("renamed", list.updates(), |state| {
        state.rooms.iter().any(|r| {
            r.id == room && r.name == "reading list" && r.topic.as_deref() == Some("Papers to read")
        })
    })
    .await?;

    let dm = s.direct_chat(&bob.session.user_id()).await?;
    wait_for("dm listed", list.updates(), |state| {
        state
            .rooms
            .iter()
            .any(|r| r.id == dm && r.kind == RoomKind::Direct)
    })
    .await?;
    assert_eq!(s.direct_chat(&bob.session.user_id()).await?, dm, "reused");

    // The directory lists people sharing a room with us, once both joined;
    // it updates in the background.
    bob.session
        .join(<&mtrx_core::RoomOrAliasId>::from(&*dm))
        .await?;
    timeout("bob in the user directory", async {
        loop {
            let found = s.search_users(&bob_name, 10).await?;
            if found.iter().any(|u| u.id == bob.session.user_id()) {
                return anyhow::Ok(());
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    })
    .await??;

    s.leave(&room).await?;
    wait_for("left", list.updates(), |state| {
        !state.rooms.iter().any(|r| r.id == room)
    })
    .await?;
    Ok(())
}

/// The message of `entries` whose body is `body`.
fn find_message<'a>(entries: &'a [TimelineEntry], body: &str) -> Option<&'a mtrx_core::Event> {
    entries.iter().find_map(|e| match e {
        TimelineEntry::Event(ev) if matches!(&ev.content, EventContent::Message(m) if m.body == body) => {
            Some(ev)
        }
        _ => None,
    })
}

fn message(event: &mtrx_core::Event) -> &mtrx_core::Message {
    match &event.content {
        EventContent::Message(m) => m,
        _ => panic!("not a message"),
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs local Synapse (testserver/run.sh)"]
async fn replies_reactions_edits_typing_receipts() -> Result<()> {
    let (_, alice) = new_user("talk").await?;
    let (_, bob) = new_user("talk").await?;
    let room = alice
        .session
        .create_room(NewRoom {
            name: Some("talk".into()),
            encrypted: true,
            invite: vec![bob.session.user_id()],
            ..Default::default()
        })
        .await?;
    join_synced(&bob, &room, "talk").await?;
    let alice_timeline = alice.session.timeline(&room).await?;
    wait_for("bob joined", alice_timeline.updates(), |entries| {
        entries
            .iter()
            .any(|e| matches!(e, TimelineEntry::Event(ev) if ev.sender == bob.session.user_id()))
    })
    .await?;
    let bob_timeline = bob.session.timeline(&room).await?;

    let question = timeout(
        "send",
        bob_timeline
            .send_markdown("Which dataset?", None)
            .await?
            .sent(),
    )
    .await??;
    wait_for("question arrives", alice_timeline.updates(), |e| {
        has_message(e, "Which dataset?")
    })
    .await?;

    // Reply.
    timeout(
        "reply",
        alice_timeline
            .send_markdown("The **new** one", Some(&question))
            .await?
            .sent(),
    )
    .await??;
    let entries = wait_for("bob sees the reply", bob_timeline.updates(), |e| {
        find_message(e, "The **new** one").is_some_and(|ev| {
            message(ev)
                .in_reply_to
                .as_ref()
                .is_some_and(|r| r.preview.is_some())
        })
    })
    .await?;
    let reply = message(find_message(&entries, "The **new** one").unwrap())
        .in_reply_to
        .clone()
        .unwrap();
    assert_eq!(reply.event_id, question);
    assert_eq!(reply.preview.as_deref(), Some("Which dataset?"));
    assert_eq!(reply.sender, Some(bob.session.user_id()));

    // Reaction, seen as ours by alice and as someone else's by bob.
    let entries = alice_timeline.current();
    let question_key = find_message(&entries, "Which dataset?")
        .unwrap()
        .key
        .clone();
    alice_timeline.toggle_reaction(&question_key, "👍").await?;
    wait_for("bob sees the reaction", bob_timeline.updates(), |e| {
        find_message(e, "Which dataset?").is_some_and(|ev| {
            message(ev).reactions
                == [mtrx_core::Reaction {
                    key: "👍".into(),
                    count: 1,
                    own: false,
                }]
        })
    })
    .await?;
    alice_timeline.toggle_reaction(&question_key, "👍").await?;
    wait_for("reaction removed", bob_timeline.updates(), |e| {
        find_message(e, "Which dataset?").is_some_and(|ev| message(ev).reactions.is_empty())
    })
    .await?;

    // Edit and delete our own message.
    let entries = alice_timeline.current();
    let own = find_message(&entries, "The **new** one").unwrap();
    assert!(own.editable);
    assert!(!find_message(&entries, "Which dataset?").unwrap().editable);
    let own_key = own.key.clone();
    alice_timeline
        .edit_markdown(&own_key, "The *newest* one")
        .await?;
    wait_for("bob sees the edit", bob_timeline.updates(), |e| {
        find_message(e, "The *newest* one").is_some_and(|ev| message(ev).edited)
    })
    .await?;
    let entries = alice_timeline.current();
    let own_key = find_message(&entries, "The *newest* one")
        .unwrap()
        .key
        .clone();
    alice_timeline.redact(&own_key).await?;
    wait_for("bob sees it deleted", bob_timeline.updates(), |e| {
        !has_message(e, "The *newest* one")
            && e.iter().any(|x| matches!(x, TimelineEntry::Event(ev) if matches!(ev.content, EventContent::Redacted)))
    })
    .await?;

    // Typing.
    let typing = bob_timeline.typing_updates();
    alice_timeline.typing(true).await?;
    wait_for("bob sees alice typing", typing, |names| names.len() == 1).await?;

    // Read receipts: bob read up to the latest event, shown with his name.
    bob_timeline.mark_read().await?;
    let bob_id = bob.session.user_id();
    wait_for("alice sees bob's receipt", alice_timeline.updates(), |e| {
        e.iter().any(|x| {
            matches!(x, TimelineEntry::Event(ev)
                if ev.read_by.iter().any(|r| r.id == bob_id && r.name.as_deref() == Some(bob_id.localpart())))
        })
    })
    .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs local Synapse (testserver/run.sh)"]
async fn members_and_notifications() -> Result<()> {
    let (_, alice) = new_user("notify").await?;
    let (_, bob) = new_user("notify").await?;
    let (_, carol) = new_user("notify").await?;
    let room = alice
        .session
        .create_room(NewRoom {
            name: Some("lab".into()),
            encrypted: true,
            invite: vec![bob.session.user_id(), carol.session.user_id()],
            ..Default::default()
        })
        .await?;
    join_synced(&bob, &room, "lab").await?;

    let members = alice.session.members(&room).await?;
    let role = |user: &Device| {
        members
            .iter()
            .find(|m| m.id == user.session.user_id())
            .map(|m| (m.role, m.invited))
    };
    assert_eq!(role(&alice), Some((mtrx_core::Role::Admin, false)));
    assert_eq!(role(&bob), Some((mtrx_core::Role::Member, false)));
    assert_eq!(role(&carol), Some((mtrx_core::Role::Member, true)));
    assert_eq!(
        members.last().map(|m| m.id.clone()),
        Some(carol.session.user_id())
    );

    // Bob's message notifies alice (decrypted), not bob himself.
    let alice_timeline = alice.session.timeline(&room).await?;
    wait_for("bob joined", alice_timeline.updates(), |entries| {
        entries
            .iter()
            .any(|e| matches!(e, TimelineEntry::Event(ev) if ev.sender == bob.session.user_id()))
    })
    .await?;
    let notifications = alice.session.notifications();
    let bob_timeline = bob.session.timeline(&room).await?;
    timeout(
        "send",
        bob_timeline
            .send_markdown("Results are in", None)
            .await?
            .sent(),
    )
    .await??;
    let notification = wait_for("notified", notifications, |n| n.room_id == room).await?;
    assert_eq!(notification.body, "Results are in");
    assert_eq!(notification.room_name, "lab");
    Ok(())
}

/// The file of the message of `entries` named `filename`.
fn find_media<'a>(entries: &'a [TimelineEntry], filename: &str) -> Option<&'a mtrx_core::Media> {
    entries.iter().find_map(|e| match e {
        TimelineEntry::Event(ev) => match &ev.content {
            EventContent::Message(m) => m.media.as_ref().filter(|m| m.filename == filename),
            _ => None,
        },
        _ => None,
    })
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs local Synapse (testserver/run.sh)"]
async fn images_and_files() -> Result<()> {
    let (_, alice) = new_user("media").await?;
    let (_, bob) = new_user("media").await?;
    let room = alice
        .session
        .create_room(NewRoom {
            name: Some("media".into()),
            encrypted: true,
            invite: vec![bob.session.user_id()],
            ..Default::default()
        })
        .await?;
    join_synced(&bob, &room, "media").await?;
    let alice_timeline = alice.session.timeline(&room).await?;
    wait_for("bob joined", alice_timeline.updates(), |entries| {
        entries
            .iter()
            .any(|e| matches!(e, TimelineEntry::Event(ev) if ev.sender == bob.session.user_id()))
    })
    .await?;

    let small = test_png(48);
    let big = test_png(1200);
    let notes = b"basal melt: 0.1 m/yr\n".to_vec();
    let dir = TempDir::new()?;
    let notes_path = dir.path().join("notes.txt");
    std::fs::write(&notes_path, &notes)?;
    alice_timeline
        .send_file_bytes("small.png", "image/png", small.clone())
        .await?;
    alice_timeline
        .send_file_bytes("big.png", "image/png", big.clone())
        .await?;
    alice_timeline.send_file(&notes_path).await?;

    let bob_timeline = bob.session.timeline(&room).await?;
    let entries = wait_for("bob sees the files", bob_timeline.updates(), |e| {
        ["small.png", "big.png", "notes.txt"]
            .iter()
            .all(|name| find_media(e, name).is_some())
    })
    .await?;
    let image = find_media(&entries, "small.png").unwrap();
    assert_eq!(image.kind, mtrx_core::MediaKind::Image);
    assert_eq!((image.width, image.height), (Some(48), Some(48)));
    assert_eq!(image.size, Some(small.len() as u64));
    assert_eq!(image.mimetype.as_deref(), Some("image/png"));
    let file = find_media(&entries, "notes.txt").unwrap();
    assert_eq!(file.kind, mtrx_core::MediaKind::File);
    assert_eq!(file.mimetype.as_deref(), Some("text/plain"));
    assert_eq!((file.width, file.height), (None, None));

    let s = &bob.session;
    assert_eq!(s.media_content(&image.source).await?, small);
    assert_eq!(s.media_preview(&image.source).await?, small);
    assert_eq!(s.media_content(&file.source).await?, notes);
    // The big image comes with a thumbnail.
    let big_media = find_media(&entries, "big.png").unwrap();
    assert_eq!(s.media_content(&big_media.source).await?, big);
    let preview = image::load_from_memory(&s.media_preview(&big_media.source).await?)?;
    assert_eq!((preview.width(), preview.height()), (600, 600));
    Ok(())
}
