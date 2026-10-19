//! A logged-in account: login, restore and logout.

use std::sync::Arc;

use matrix_sdk::{
    Client, SessionChange,
    authentication::oauth::{
        ClientId, OAuthSession,
        registration::{ApplicationType, ClientMetadata, Localized, OAuthGrantType},
    },
    encryption::{BackupDownloadStrategy, EncryptionSettings, verification::VerificationRequest},
    ruma::{OwnedDeviceId, OwnedUserId, serde::Raw},
    store::RoomLoadSettings,
    utils::{
        UrlOrQuery,
        local_server::{LocalServerBuilder, LocalServerResponse},
    },
};
use matrix_sdk_ui::sync_service::SyncService;
use tokio::{
    sync::{Mutex, broadcast},
    task::JoinHandle,
};
use tracing::warn;
use url::Url;

use crate::{
    Error, Profile, Result,
    profile::{StoredAuth, StoredSession},
    scheduled::Schedule,
};

const DEVICE_DISPLAY_NAME: &str = "Mtrx";
/// Shown by the server's login page as the app's home page.
const CLIENT_URI: &str = "https://alex-robinson.github.io/";
const BROWSER_DONE_PAGE: &str = "<!doctype html><title>Mtrx</title>\
    <p style=\"font-family: sans-serif\">Signed in. You can close this tab and return to Mtrx.</p>";

/// A logged-in Matrix account bound to a [`Profile`].
pub struct Session {
    pub(crate) client: Client,
    pub(crate) sync: Arc<SyncService>,
    pub(crate) verification_tx: broadcast::Sender<VerificationRequest>,
    pub(crate) room_keys: Arc<crate::encryption::RoomKeys>,
    /// Serializes enqueueing messages, see [`crate::timeline::send_markdown`].
    pub(crate) send_lock: Arc<Mutex<()>>,
    pub(crate) schedule: Arc<Schedule>,
    /// Set once [`Session::start_scheduler`] ran.
    pub(crate) scheduler: std::sync::Mutex<Option<AbortOnDrop>>,
    pub(crate) notifications: broadcast::Sender<crate::Notification>,
    profile: Profile,
    _save_tokens: AbortOnDrop,
}

pub(crate) struct AbortOnDrop(pub(crate) JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
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
        Self::login_with(profile, homeserver, async |client| {
            client
                .matrix_auth()
                .login_username(user, password)
                .initial_device_display_name(DEVICE_DISPLAY_NAME)
                .await?;
            Ok(())
        })
        .await
    }

    /// Log in through the server's web page (OAuth 2.0, e.g. matrix.org).
    /// `open_url` must show the given URL to the user, normally in their
    /// browser; the login completes when the browser is redirected back to a
    /// temporary local web server. Dropping the future cancels the login.
    pub async fn login_oauth(
        profile: Profile,
        homeserver: &str,
        open_url: impl FnOnce(Url),
    ) -> Result<Self> {
        Self::login_with(profile, homeserver, async |client| {
            let (redirect_uri, redirect) = LocalServerBuilder::new()
                .response(LocalServerResponse::Html(BROWSER_DONE_PAGE.into()))
                .spawn()
                .await?;
            let oauth = client.oauth();
            let authorization = oauth
                .login(
                    redirect_uri.clone(),
                    None,
                    Some(client_metadata(redirect_uri)?.into()),
                    None,
                )
                .build()
                .await?;
            open_url(authorization.url);
            let query = redirect.await.ok_or(Error::LoginCancelled)?;
            oauth.finish_login(UrlOrQuery::Query(query.0)).await?;
            Ok(())
        })
        .await
    }

    /// Whether the server offers [`Session::login_oauth`].
    pub async fn supports_oauth(homeserver: &str) -> Result<bool> {
        let client = client_builder(homeserver).build().await?;
        Ok(client.oauth().server_metadata().await.is_ok())
    }

    /// Shared part of the logins: fresh store, `authenticate`, persist.
    async fn login_with(
        profile: Profile,
        homeserver: &str,
        authenticate: impl AsyncFnOnce(&Client) -> Result<()>,
    ) -> Result<Self> {
        if profile.load_session()?.is_some() {
            return Err(Error::AlreadyLoggedIn(profile.name().to_owned()));
        }
        // Leftovers from an earlier failed login would not open with a new passphrase.
        profile.wipe()?;

        let store_passphrase = random_passphrase();
        let result = async {
            let client = build_client(&profile, homeserver, &store_passphrase).await?;
            authenticate(&client).await?;
            // Cross-signing and backup setup run in the background; finish them
            // before the caller can exit or start syncing.
            client
                .encryption()
                .wait_for_e2ee_initialization_tasks()
                .await;
            profile.save_session(&StoredSession {
                homeserver: client.homeserver().to_string(),
                auth: stored_auth(&client),
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
        match stored.auth {
            StoredAuth::Password(session) => {
                client
                    .matrix_auth()
                    .restore_session(session, RoomLoadSettings::default())
                    .await?
            }
            StoredAuth::OAuth { client_id, session } => {
                let session = OAuthSession {
                    client_id: ClientId::new(client_id),
                    user: session,
                };
                client
                    .oauth()
                    .restore_session(session, RoomLoadSettings::default())
                    .await?
            }
        }
        client
            .encryption()
            .wait_for_e2ee_initialization_tasks()
            .await;
        Self::new(client, profile).await
    }

    async fn new(client: Client, profile: Profile) -> Result<Self> {
        let sync = SyncService::builder(client.clone())
            .build()
            .await
            .map_err(|e| Error::Sync(e.to_string()))?;
        let verification_tx = crate::encryption::register_handlers(&client);
        let notifications = crate::notifications::register(&client).await;
        let save_tokens = AbortOnDrop(tokio::spawn(save_refreshed_tokens(
            client.clone(),
            profile.clone(),
        )));
        let send_lock = Arc::new(Mutex::new(()));
        let schedule = Arc::new(Schedule::load(client.clone(), send_lock.clone()).await?);
        Ok(Self {
            client,
            sync: Arc::new(sync),
            verification_tx,
            room_keys: Default::default(),
            send_lock,
            schedule,
            scheduler: Default::default(),
            notifications,
            profile,
            _save_tokens: save_tokens,
        })
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
    /// was already revoked. The session must not be used afterwards.
    pub async fn logout(&self) -> Result<()> {
        if let Err(e) = self.client.logout().await {
            warn!("server-side logout failed, removing local data anyway: {e}");
        }
        self.sync.stop().await;
        self.profile.forget_session()?;
        // Store files may still be open (e.g. on Windows they then cannot be
        // deleted); the next login removes anything left behind.
        if let Err(e) = self.profile.remove_data() {
            warn!("could not delete all local data: {e}");
        }
        Ok(())
    }

    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    pub fn user_id(&self) -> OwnedUserId {
        self.client
            .user_id()
            .expect("session is logged in")
            .to_owned()
    }

    pub fn device_id(&self) -> OwnedDeviceId {
        self.client
            .device_id()
            .expect("session is logged in")
            .to_owned()
    }

    pub fn homeserver(&self) -> String {
        self.client.homeserver().to_string()
    }
}

fn client_builder(homeserver: &str) -> matrix_sdk::ClientBuilder {
    let builder = Client::builder();
    // A full URL is used as-is; a bare server name goes through .well-known discovery.
    if homeserver.contains("://") {
        builder.homeserver_url(homeserver)
    } else {
        builder.server_name_or_homeserver_url(homeserver)
    }
}

async fn build_client(profile: &Profile, homeserver: &str, passphrase: &str) -> Result<Client> {
    Ok(client_builder(homeserver)
        .sqlite_store(profile.store_dir(), Some(passphrase))
        .handle_refresh_tokens()
        .with_encryption_settings(EncryptionSettings {
            auto_enable_cross_signing: true,
            backup_download_strategy: BackupDownloadStrategy::AfterDecryptionFailure,
            auto_enable_backups: true,
        })
        .build()
        .await?)
}

fn stored_auth(client: &Client) -> StoredAuth {
    match client.oauth().full_session() {
        Some(session) => StoredAuth::OAuth {
            client_id: session.client_id.as_str().to_owned(),
            session: session.user,
        },
        None => StoredAuth::Password(client.matrix_auth().session().expect("logged in")),
    }
}

/// Access tokens expire and get refreshed by the SDK; keep the stored copy current.
async fn save_refreshed_tokens(client: Client, profile: Profile) {
    let mut changes = client.subscribe_to_session_changes();
    loop {
        match changes.recv().await {
            Ok(SessionChange::TokensRefreshed) => {
                let saved = profile.load_session().and_then(|stored| match stored {
                    Some(mut stored) => {
                        stored.auth = stored_auth(&client);
                        profile.save_session(&stored)
                    }
                    None => Ok(()),
                });
                if let Err(e) = saved {
                    warn!("could not save refreshed tokens: {e}");
                }
            }
            Ok(SessionChange::UnknownToken(_)) => {
                warn!("the server no longer accepts this session")
            }
            Err(broadcast::error::RecvError::Lagged(_)) => {}
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}

/// How we present ourselves to the server's login page.
fn client_metadata(redirect_uri: Url) -> Result<Raw<ClientMetadata>> {
    let client_uri = Url::parse(CLIENT_URI).expect("valid URL");
    let mut metadata = ClientMetadata::new(
        ApplicationType::Native,
        vec![OAuthGrantType::AuthorizationCode {
            redirect_uris: vec![redirect_uri],
        }],
        Localized::new(client_uri, None),
    );
    metadata.client_name = Some(Localized::new(DEVICE_DISPLAY_NAME.to_owned(), None));
    Ok(Raw::new(&metadata)?)
}

fn random_passphrase() -> String {
    let bytes: [u8; 32] = rand::random();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
