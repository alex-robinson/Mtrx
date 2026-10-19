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

use std::{future::Future, time::Duration};

use anyhow::{Context, Result, bail};
use futures_util::{Stream, StreamExt};
use mtrx_core::{
    EventContent, MembershipChange, NewRoom, Profile, RecoveryStatus, RoomKind, SecretStore,
    Session, TimelineEntry, VerificationState,
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
        alice_timeline.send_markdown("hello **bob**").await?.sent(),
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
            .send_markdown("written before recovery")
            .await?
            .sent(),
    )
    .await??;
    let key = first.session.enable_recovery().await?;
    assert_eq!(first.session.recovery_status(), RecoveryStatus::Enabled);

    let second = login(&name).await?;
    assert!(!second.session.is_device_verified().await?);
    second.session.room_list().await?.wait_loaded().await;
    second.session.recover(&key).await?;
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
