//! `mtrx`: command-line Matrix client built on `mtrx-core`.

// matrix-sdk futures are deeply nested.
#![recursion_limit = "256"]

mod rooms;
mod timeline;

use std::{
    collections::HashMap,
    io::{BufRead, Read},
    path::PathBuf,
    time::Duration,
};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use futures_util::StreamExt;
use mtrx_core::{
    NewRoom, OwnedRoomId, OwnedUserId, Profile, RoomList, RoomTimeline, SecretStore, Session,
    TimelineEntry,
};

/// How long one-shot commands wait for the first sync.
const SYNC_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Parser)]
#[command(name = "mtrx", version, about = "Matrix client")]
struct Cli {
    /// Account profile to use.
    #[arg(long, global = true, env = "MTRX_PROFILE", default_value = "default")]
    profile: String,
    /// Root directory for profiles (default: platform data directory).
    #[arg(long, global = true, env = "MTRX_DATA_DIR")]
    data_dir: Option<PathBuf>,
    /// Where to keep access tokens and store passphrases.
    #[arg(long, global = true, env = "MTRX_SECRET_STORE", default_value = "keyring")]
    secret_store: SecretStoreArg,
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, ValueEnum)]
enum SecretStoreArg {
    /// OS credential store.
    Keyring,
    /// Plain-text file in the profile directory (development only).
    File,
}

#[derive(Subcommand)]
enum Command {
    /// Log in with a password.
    Login {
        /// Server name (matrix.org) or URL (http://localhost:8008).
        #[arg(long)]
        homeserver: String,
        /// User name or full user ID.
        #[arg(long)]
        user: String,
        /// Read the password from the first line of stdin instead of prompting.
        #[arg(long)]
        password_stdin: bool,
    },
    /// Log out and delete local data for the profile.
    Logout,
    /// Show the logged-in account.
    Whoami,
    /// List rooms, grouped by space.
    Rooms {
        /// Show room IDs.
        #[arg(long)]
        ids: bool,
    },
    /// Create a room.
    CreateRoom {
        name: String,
        /// Create an unencrypted room.
        #[arg(long)]
        unencrypted: bool,
        /// Users to invite.
        #[arg(long)]
        invite: Vec<OwnedUserId>,
    },
    /// Create a space.
    CreateSpace { name: String },
    /// Join a room by ID or alias, or accept an invite.
    Join { room: String },
    /// Leave a room.
    Leave { room: String },
    /// Invite a user to a room.
    Invite { room: String, user: OwnedUserId },
    /// Add a room to a space.
    SpaceAdd { space: String, room: String },
    /// Print recent messages of a room.
    Read {
        room: String,
        /// Number of events to show.
        #[arg(short = 'n', default_value_t = 20)]
        count: usize,
    },
    /// Send a Markdown message (from the argument, or stdin if omitted).
    Send { room: String, text: Option<String> },
    /// Follow a room's messages, or unread counts of all rooms, until Ctrl-C.
    Watch { room: Option<String> },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,matrix_sdk=error,matrix_sdk::http_client=off,matrix_sdk::latest_events=off".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let secrets = match cli.secret_store {
        SecretStoreArg::Keyring => SecretStore::Keyring,
        SecretStoreArg::File => SecretStore::File,
    };
    let profile = Profile::new(&cli.profile, cli.data_dir.as_deref(), secrets)?;

    match cli.command {
        Command::Login { homeserver, user, password_stdin } => {
            let password = if password_stdin {
                let mut line = String::new();
                std::io::stdin().lock().read_line(&mut line)?;
                line.trim_end_matches(['\r', '\n']).to_owned()
            } else {
                rpassword::prompt_password("Password: ")?
            };
            let session = Session::login(profile, &homeserver, &user, &password)
                .await
                .context("login failed")?;
            println!("Logged in as {} (device {})", session.user_id(), session.device_id());
        }
        Command::Logout => {
            Session::restore(profile).await?.logout().await?;
            println!("Logged out");
        }
        Command::Whoami => {
            let session = Session::restore(profile).await?;
            println!("user:       {}", session.user_id());
            println!("device:     {}", session.device_id());
            println!("homeserver: {}", session.homeserver());
            println!("profile:    {}", session.profile().dir().display());
        }
        Command::Rooms { ids } => {
            let (session, list) = synced(profile).await?;
            rooms::print_tree(&list.current().rooms, ids);
            session.stop_sync().await;
        }
        Command::CreateRoom { name, unencrypted, invite } => {
            let session = Session::restore(profile).await?;
            let new = NewRoom { name: Some(name), encrypted: !unencrypted, invite, ..Default::default() };
            println!("{}", session.create_room(new).await?);
        }
        Command::CreateSpace { name } => {
            let session = Session::restore(profile).await?;
            let new = NewRoom { name: Some(name), space: true, ..Default::default() };
            println!("{}", session.create_room(new).await?);
        }
        Command::Join { room } => {
            let session = Session::restore(profile).await?;
            let room = room.as_str().try_into().context("expected a room ID or alias")?;
            println!("{}", session.join(room).await?);
        }
        Command::Leave { room } => {
            let (session, _list) = synced(profile).await?;
            session.leave(&session.find_room(&room).await?).await?;
            session.stop_sync().await;
        }
        Command::Invite { room, user } => {
            let (session, _list) = synced(profile).await?;
            session.invite(&session.find_room(&room).await?, &user).await?;
            session.stop_sync().await;
        }
        Command::SpaceAdd { space, room } => {
            let (session, _list) = synced(profile).await?;
            let space: OwnedRoomId = session.find_room(&space).await?;
            session.add_to_space(&space, &session.find_room(&room).await?).await?;
            session.stop_sync().await;
        }
        Command::Read { room, count } => {
            let (session, _list) = synced(profile).await?;
            let timeline = session.timeline(&session.find_room(&room).await?).await?;
            load_events(&timeline, count).await?;
            let entries = timeline.current();
            let skip = entries.len().saturating_sub(count);
            for entry in &entries[skip..] {
                println!("{}", timeline::entry_line(entry));
            }
            session.stop_sync().await;
        }
        Command::Send { room, text } => {
            let text = match text {
                Some(text) => text,
                None => {
                    let mut text = String::new();
                    std::io::stdin().read_to_string(&mut text)?;
                    text
                }
            };
            let (session, _list) = synced(profile).await?;
            let timeline = session.timeline(&session.find_room(&room).await?).await?;
            let pending = timeline.send_markdown(&text).await?;
            let sent = tokio::time::timeout(SYNC_TIMEOUT, pending.sent()).await;
            session.stop_sync().await;
            sent.context("timed out sending")??;
        }
        Command::Watch { room: Some(room) } => {
            let (session, _list) = synced(profile).await?;
            let timeline = session.timeline(&session.find_room(&room).await?).await?;
            load_events(&timeline, 10).await?;
            watch_timeline(&timeline).await;
            session.stop_sync().await;
        }
        Command::Watch { room: None } => {
            let (session, list) = synced(profile).await?;
            watch_rooms(&list).await;
            session.stop_sync().await;
        }
    }
    Ok(())
}

/// Paginate until at least `count` events are loaded or the room start is reached.
async fn load_events(timeline: &RoomTimeline, count: usize) -> Result<()> {
    let events = |t: &RoomTimeline| {
        t.current().iter().filter(|e| matches!(e, TimelineEntry::Event(_))).count()
    };
    while events(timeline) < count {
        if timeline.paginate_back(count.max(20) as u16).await? {
            break;
        }
    }
    Ok(())
}

/// Print the timeline, then new and changed events, until Ctrl-C.
async fn watch_timeline(timeline: &RoomTimeline) {
    let mut seen: HashMap<String, String> = HashMap::new();
    let mut updates = timeline.updates();
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => return,
            entries = updates.next() => {
                let Some(entries) = entries else { return };
                for entry in entries {
                    let TimelineEntry::Event(event) = &entry else { continue };
                    let line = timeline::event_line(event);
                    match seen.insert(event.id.clone(), line.clone()) {
                        None => println!("{line}"),
                        Some(old) if old != line => println!("~ {line}"),
                        Some(_) => {}
                    }
                }
            }
        }
    }
}

/// Print rooms whose unread count changes, until Ctrl-C.
async fn watch_rooms(list: &RoomList) {
    let mut unread: HashMap<OwnedRoomId, u64> = HashMap::new();
    let mut updates = list.updates();
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => return,
            state = updates.next() => {
                let Some(state) = state else { return };
                for room in state.rooms {
                    if unread.insert(room.id.clone(), room.unread_messages) != Some(room.unread_messages) {
                        println!("{}: {} unread", room.name, room.unread_messages);
                    }
                }
            }
        }
    }
}

/// Restore the session and wait until the room list is loaded.
async fn synced(profile: Profile) -> Result<(Session, RoomList)> {
    let session = Session::restore(profile).await?;
    session.start_sync().await;
    let list = session.room_list().await?;
    tokio::time::timeout(SYNC_TIMEOUT, list.wait_loaded())
        .await
        .context("timed out waiting for sync")?;
    Ok((session, list))
}
