//! UI snapshots against the local Synapse (`testserver/run.sh`):
//! `cargo test -p mtrx-app -- --ignored`, images land in `target/ui-snapshots/`.

use std::{path::PathBuf, sync::Arc, time::Duration};

use futures_util::StreamExt;
use iced::Size;
use iced_test::simulator::Simulator;
use mtrx_core::{
    EventContent, NewRoom, Profile, RoomOrAliasId, SecretStore, Session, TimelineEntry,
    test_support::{PASSWORD, homeserver, register, unique_name},
};
use tempfile::TempDir;

use super::*;
use crate::fonts;

const TIMEOUT: Duration = Duration::from_secs(30);

fn snapshot_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-snapshots")
}

async fn user(prefix: &str, dir: &TempDir) -> (String, Session) {
    let name = unique_name(prefix);
    register(&name).await.unwrap();
    let profile = Profile::new(prefix, Some(dir.path()), SecretStore::File).unwrap();
    let session = Session::login(profile, &homeserver(), &name, PASSWORD)
        .await
        .unwrap();
    session.start_sync().await;
    (name, session)
}

fn snapshot(app: &App, name: &str) {
    // Snapshots are for looking at, not regression baselines: always rewrite.
    // iced_test appends the renderer name, e.g. `login-wgpu.png`.
    let path = snapshot_dir().join(name);
    for entry in std::fs::read_dir(snapshot_dir())
        .into_iter()
        .flatten()
        .flatten()
    {
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with(&format!("{name}-"))
        {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    let mut ui = Simulator::with_size(fonts::settings(), Size::new(1200.0, 800.0), app.view());
    let snapshot = ui.snapshot(&app.theme()).unwrap();
    assert!(snapshot.matches_image(&path).unwrap());
}

fn app(screen: Screen, mode: Mode) -> App {
    App {
        profile: Profile::new("ui", None, SecretStore::File).unwrap(),
        screen,
        mode,
        window_width: 1200.0,
        scale: 2.0,
        math: Math::new(),
    }
}

#[test]
fn login_screen() {
    snapshot(&app(Screen::Login(Login::default()), Mode::Light), "login");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs local Synapse (testserver/run.sh)"]
async fn main_screen_with_math() {
    let dir = TempDir::new().unwrap();
    let (_, alice) = user("alice", &dir).await;
    let (_, bob) = user("bob", &dir).await;

    let space = alice
        .create_room(NewRoom {
            name: Some("Climate".into()),
            space: true,
            ..Default::default()
        })
        .await
        .unwrap();
    let room = alice
        .create_room(NewRoom {
            name: Some("ice-sheets".into()),
            encrypted: true,
            invite: vec![bob.user_id()],
            ..Default::default()
        })
        .await
        .unwrap();
    let general = alice
        .create_room(NewRoom {
            name: Some("general".into()),
            encrypted: true,
            ..Default::default()
        })
        .await
        .unwrap();
    alice.add_to_space(&space, &room).await.unwrap();
    alice.add_to_space(&space, &general).await.unwrap();

    bob.room_list().await.unwrap().wait_loaded().await;
    let bob_list = bob.room_list().await.unwrap();
    let mut updates = std::pin::pin!(bob_list.updates());
    tokio::time::timeout(TIMEOUT, async {
        while let Some(state) = updates.next().await {
            if state.rooms.iter().any(|r| r.id == room) {
                break;
            }
        }
    })
    .await
    .unwrap();
    bob.join(<&RoomOrAliasId>::from(&*room)).await.unwrap();

    let alice_timeline = alice.timeline(&room).await.unwrap();
    // Wait for bob's join so he receives the room keys.
    let mut entries = std::pin::pin!(alice_timeline.updates());
    tokio::time::timeout(TIMEOUT, async {
        while let Some(entries) = entries.next().await {
            if entries
                .iter()
                .any(|e| matches!(e, TimelineEntry::Event(ev) if ev.sender == bob.user_id()))
            {
                break;
            }
        }
    })
    .await
    .unwrap();
    for text in [
        "Morning! Draft of the **ice flux** section is up.",
        r"The basal melt rate follows $\dot m = \frac{G + \tau_b u_b}{\rho_i L}$ where $G$ is the geothermal flux.",
        "Steady state requires\n$$\\frac{\\partial H}{\\partial t} = -\\nabla \\cdot (H \\bar{\\mathbf{u}}) + \\dot a - \\dot m = 0$$",
    ] {
        tokio::time::timeout(
            TIMEOUT,
            alice_timeline.send_markdown(text).await.unwrap().sent(),
        )
        .await
        .unwrap()
        .unwrap();
    }
    let bob_timeline = bob.timeline(&room).await.unwrap();
    tokio::time::timeout(
        TIMEOUT,
        bob_timeline
            .send_markdown("Looks good, but check the sign of $\\dot m$ in eq. 2 🙂")
            .await
            .unwrap()
            .sent(),
    )
    .await
    .unwrap()
    .unwrap();

    // Alice's view of the room once bob's message arrived.
    let list = Arc::new(alice.room_list().await.unwrap());
    let rooms = tokio::time::timeout(TIMEOUT, async {
        let mut updates = std::pin::pin!(list.updates());
        while let Some(state) = updates.next().await {
            if state.loaded
                && state
                    .rooms
                    .iter()
                    .any(|r| r.id == room && r.parents.contains(&space))
            {
                return state;
            }
        }
        unreachable!()
    })
    .await
    .unwrap();
    let timeline = Arc::new(alice.timeline(&room).await.unwrap());
    while !timeline.paginate_back(50).await.unwrap() {}
    let entries = tokio::time::timeout(TIMEOUT, async {
        let mut updates = std::pin::pin!(timeline.updates());
        while let Some(entries) = updates.next().await {
            let from_bob = entries.iter().any(|e| {
                matches!(e, TimelineEntry::Event(ev)
                    if ev.sender == bob.user_id() && matches!(ev.content, EventContent::Message(_)))
            });
            if from_bob {
                return entries;
            }
        }
        unreachable!()
    })
    .await
    .unwrap();

    for mode in [Mode::Light, Mode::Dark] {
        let session = Arc::new(Session::restore(alice.profile().clone()).await.unwrap());
        let mut main = Main::new(session);
        main.rooms = rooms.clone();
        main.space = Some(space.clone());
        main.verified = Some(true);
        let mut open = Room::new(room.clone(), timeline.clone());
        open.entries = entries.clone();
        open.at_start = true;
        main.room = Some(open);

        let mut app = app(Screen::Main(Box::new(main)), mode);
        let params = app.math_params();
        if let Screen::Main(main) = &app.screen {
            for document in main.room.as_ref().unwrap().math_documents() {
                app.math.render_now(document, &params);
            }
        }
        let name = if mode == Mode::Dark {
            "main-dark"
        } else {
            "main-light"
        };
        snapshot(&app, name);
    }
}

/// Timings for a big encrypted room opened on a new, recovered device:
/// `cargo test --release -p mtrx-app big_room -- --ignored --nocapture`
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs local Synapse (testserver/run.sh); slow"]
async fn big_room_benchmark() {
    const MESSAGES: usize = 1500;
    let dir = TempDir::new().unwrap();
    let (name, first) = user("bench", &dir).await;
    let room = first
        .create_room(NewRoom {
            name: Some("big".into()),
            encrypted: true,
            ..Default::default()
        })
        .await
        .unwrap();
    let timeline = first.timeline(&room).await.unwrap();
    let start = std::time::Instant::now();
    let mut last = None;
    for i in 0..MESSAGES {
        let text = if i % 10 == 0 {
            format!("message {i} with math $x_{{{i}}} = \\frac{{a}}{{b}}$")
        } else {
            format!("message {i}: some **text** to read")
        };
        last = Some(timeline.send_markdown(&text).await.unwrap());
    }
    tokio::time::timeout(Duration::from_secs(300), last.unwrap().sent())
        .await
        .unwrap()
        .unwrap();
    println!("sent {MESSAGES} messages in {:.1?}", start.elapsed());
    let key = first.enable_recovery().await.unwrap();

    // New device, recovered: like a fresh install.
    let dir2 = TempDir::new().unwrap();
    let profile = Profile::new("second", Some(dir2.path()), SecretStore::File).unwrap();
    let second = Session::login(profile, &homeserver(), &name, PASSWORD)
        .await
        .unwrap();
    second.start_sync().await;
    second.room_list().await.unwrap().wait_loaded().await;
    second.recover(&key, |_| {}).await.unwrap();

    let start = std::time::Instant::now();
    let timeline = Arc::new(second.timeline(&room).await.unwrap());
    let mut updates = std::pin::pin!(timeline.updates());
    let paginate = {
        let timeline = timeline.clone();
        tokio::spawn(async move { while !timeline.paginate_back(50).await.unwrap() {} })
    };
    let mut count = 0;
    let decrypted = |entries: &[TimelineEntry]| {
        let messages = entries
            .iter()
            .filter(|e| matches!(e, TimelineEntry::Event(ev) if matches!(ev.content, EventContent::Message(_))))
            .count();
        let utd = entries
            .iter()
            .filter(|e| matches!(e, TimelineEntry::Event(ev) if ev.content == EventContent::UnableToDecrypt))
            .count();
        (messages, utd)
    };
    let mut paginated_at = None;
    while let Some(entries) = tokio::time::timeout(Duration::from_secs(120), updates.next())
        .await
        .unwrap()
    {
        count += 1;
        if paginate.is_finished() && paginated_at.is_none() {
            paginated_at = Some(start.elapsed());
        }
        let (messages, utd) = decrypted(&entries);
        if messages == MESSAGES && utd == 0 {
            break;
        }
    }
    println!(
        "new device: history loaded after {:.1?}, all decrypted after {:.1?} ({count} timeline updates)",
        paginated_at.unwrap_or_default(),
        start.elapsed()
    );

    let start = std::time::Instant::now();
    for _ in 0..100 {
        std::hint::black_box(timeline.current());
    }
    println!(
        "timeline snapshot ({} entries): {:.2?} each",
        timeline.current().len(),
        start.elapsed() / 100
    );

    // View cost with every entry in the room view.
    let session = Arc::new(second);
    let mut main = Main::new(session);
    let mut open = Room::new(room.clone(), timeline.clone());
    open.at_start = true;
    main.room = Some(open);
    let app = app(Screen::Main(Box::new(main)), Mode::Light);
    let start = std::time::Instant::now();
    for _ in 0..5 {
        let mut ui = Simulator::with_size(fonts::settings(), Size::new(1200.0, 800.0), app.view());
        std::hint::black_box(ui.snapshot(&app.theme()).unwrap());
    }
    println!(
        "view + layout + draw: {:.1?} per frame",
        start.elapsed() / 5
    );
}
