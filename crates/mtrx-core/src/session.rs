//! A logged-in account: login, restore and logout.

use std::sync::Arc;

use matrix_sdk::{
    Client,
    encryption::{BackupDownloadStrategy, EncryptionSettings},
    ruma::{OwnedDeviceId, OwnedUserId},
    store::RoomLoadSettings,
};
use matrix_sdk_ui::sync_service::SyncService;
use tokio::sync::Mutex;
use tracing::warn;

use crate::{
    Error, Profile, Result,
    profile::StoredSession,
};

const DEVICE_DISPLAY_NAME: &str = "Mtrx";

/// A logged-in Matrix account bound to a [`Profile`].
pub struct Session {
    pub(crate) client: Client,
    pub(crate) sync: Arc<SyncService>,
    /// Serializes enqueueing messages, see [`crate::RoomTimeline::send_markdown`].
    pub(crate) send_lock: Arc<Mutex<()>>,
    profile: Profile,
}

impl Session {
    /// Log in with a password and persist the session in `profile`.
    ///
    /// `homeserver` may be a server name (`matrix.org`) or a URL.
    pub async fn login(
        profile: Profile,
        homeserver: &str,
        user: &str,
        password: &str,
    ) -> Result<Self> {
        if profile.load_session()?.is_some() {
            return Err(Error::AlreadyLoggedIn(profile.name().to_owned()));
        }
        // Leftovers from an earlier failed login would not open with a new passphrase.
        profile.wipe()?;

        let store_passphrase = random_passphrase();
        let result = async {
            let client = build_client(&profile, homeserver, &store_passphrase).await?;
            client
                .matrix_auth()
                .login_username(user, password)
                .initial_device_display_name(DEVICE_DISPLAY_NAME)
                .await?;
            // Cross-signing and backup setup run in the background; finish them
            // before the caller can exit or start syncing.
            client.encryption().wait_for_e2ee_initialization_tasks().await;
            let session = client.matrix_auth().session().expect("session exists after login");
            profile.save_session(&StoredSession {
                homeserver: client.homeserver().to_string(),
                session,
                store_passphrase,
            })?;
            Ok(client)
        }
        .await;

        match result {
            Ok(client) => Self::new(client, profile).await,
            Err(e) => {
                profile.wipe()?;
                Err(e)
            }
        }
    }

    /// Reopen the session previously saved in `profile`.
    pub async fn restore(profile: Profile) -> Result<Self> {
        let stored = profile
            .load_session()?
            .ok_or_else(|| Error::NotLoggedIn(profile.name().to_owned()))?;
        let client = build_client(&profile, &stored.homeserver, &stored.store_passphrase).await?;
        client.matrix_auth().restore_session(stored.session, RoomLoadSettings::default()).await?;
        client.encryption().wait_for_e2ee_initialization_tasks().await;
        Self::new(client, profile).await
    }

    async fn new(client: Client, profile: Profile) -> Result<Self> {
        let sync = SyncService::builder(client.clone())
            .build()
            .await
            .map_err(|e| Error::Sync(e.to_string()))?;
        Ok(Self { client, sync: Arc::new(sync), send_lock: Default::default(), profile })
    }

    /// Start syncing in the background. Live views ([`crate::RoomList`], ...)
    /// only update while sync runs.
    pub async fn start_sync(&self) {
        self.sync.start().await;
    }

    /// Stop syncing and persist the sync position.
    pub async fn stop_sync(&self) {
        self.sync.stop().await;
    }

    /// Log out on the server and delete all local data of the profile.
    ///
    /// Local data is deleted even if the server is unreachable or the token
    /// was already revoked.
    pub async fn logout(self) -> Result<()> {
        if let Err(e) = self.client.matrix_auth().logout().await {
            warn!("server-side logout failed, removing local data anyway: {e}");
        }
        self.sync.stop().await;
        let Self { client, sync, profile, .. } = self;
        drop(sync);
        drop(client);
        profile.wipe()
    }

    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    pub fn user_id(&self) -> OwnedUserId {
        self.client.user_id().expect("session is logged in").to_owned()
    }

    pub fn device_id(&self) -> OwnedDeviceId {
        self.client.device_id().expect("session is logged in").to_owned()
    }

    pub fn homeserver(&self) -> String {
        self.client.homeserver().to_string()
    }
}

async fn build_client(profile: &Profile, homeserver: &str, passphrase: &str) -> Result<Client> {
    let builder = Client::builder();
    // A full URL is used as-is; a bare server name goes through .well-known discovery.
    let builder = if homeserver.contains("://") {
        builder.homeserver_url(homeserver)
    } else {
        builder.server_name_or_homeserver_url(homeserver)
    };
    Ok(builder
        .sqlite_store(profile.store_dir(), Some(passphrase))
        .with_encryption_settings(EncryptionSettings {
            auto_enable_cross_signing: true,
            backup_download_strategy: BackupDownloadStrategy::AfterDecryptionFailure,
            auto_enable_backups: true,
        })
        .build()
        .await?)
}

fn random_passphrase() -> String {
    let bytes: [u8; 32] = rand::random();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
