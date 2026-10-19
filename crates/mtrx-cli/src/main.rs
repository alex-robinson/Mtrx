//! `mtrx`: command-line Matrix client built on `mtrx-core`.

use std::{io::BufRead, path::PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use mtrx_core::{Profile, SecretStore, Session};

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
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,matrix_sdk=error,matrix_sdk::http_client=off".into()),
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
    }
    Ok(())
}
