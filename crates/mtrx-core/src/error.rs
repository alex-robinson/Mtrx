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

    #[error(transparent)]
    Matrix(#[from] matrix_sdk::Error),
    #[error(transparent)]
    Http(#[from] matrix_sdk::HttpError),
    #[error(transparent)]
    ClientBuild(#[from] matrix_sdk::ClientBuildError),
    #[error("secret storage: {0}")]
    Keyring(#[from] keyring::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
