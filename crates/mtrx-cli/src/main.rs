//! `mtrx`: command-line Matrix client built on `mtrx-core`.

mod rooms;

use std::{io::BufRead, path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use mtrx_core::{NewRoom, OwnedRoomId, OwnedUserId, Profile, RoomList, SecretStore, Session};

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
    }
    Ok(())
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
