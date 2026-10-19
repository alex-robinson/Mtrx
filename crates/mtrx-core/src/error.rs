use thiserror::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid profile name `{0}` (use letters, digits, `-` and `_`)")]
    InvalidProfileName(String),
    #[error("could not determine a data directory for this platform")]
    NoDataDir,
    #[error("profile `{0}` is not logged in")]
    NotLoggedIn(String),
    #[error("profile `{0}` is already logged in; log out first")]
    AlreadyLoggedIn(String),
    #[error("login was cancelled")]
    LoginCancelled,
    #[error(transparent)]
    OAuth(#[from] matrix_sdk::authentication::oauth::OAuthError),
    #[error("no room matches `{0}`")]
    RoomNotFound(String),
    #[error("`{0}` matches several rooms: {1:?}")]
    AmbiguousRoom(String, Vec<matrix_sdk::ruma::OwnedRoomId>),
    #[error("sync: {0}")]
    Sync(String),
    #[error("room list: {0}")]
    RoomList(String),
    #[error("timeline: {0}")]
    Timeline(String),
    #[error("verification: {0}")]
    Verification(String),
    #[error("no scheduled message `{0}`")]
    ScheduledNotFound(String),
    #[error("the send time is in the past")]
    SendTimeInPast,
    #[error("not a valid call service link: `{0}`")]
    InvalidCallService(String),
    #[error("permissions: {0}")]
    Permissions(String),
    #[error("invalid media type `{0}`")]
    InvalidMediaType(String),

    #[error(transparent)]
    Matrix(#[from] matrix_sdk::Error),
    #[error(transparent)]
    Http(#[from] matrix_sdk::HttpError),
    #[error(transparent)]
    Recovery(#[from] matrix_sdk::encryption::recovery::RecoveryError),
    #[error(transparent)]
    Store(#[from] matrix_sdk::StoreError),
    #[error(transparent)]
    Crypto(#[from] matrix_sdk::encryption::CryptoStoreError),
    #[error(transparent)]
    ClientBuild(#[from] matrix_sdk::ClientBuildError),
    #[error("secret storage: {0}")]
    Keyring(#[from] keyring::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("Google: {0}")]
    Google(String),
    #[error("not signed in to Google")]
    GoogleNotSignedIn,
    #[error("Google sign-in expired or was revoked; sign in to Google again")]
    GoogleSignInExpired,
}
