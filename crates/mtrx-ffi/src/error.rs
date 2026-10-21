//! The errors the apps see.

/// Why a call failed. Show `message`s (and [`MtrxError::LoginCancelled`]
/// not at all); match the others to react.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum MtrxError {
    /// No saved session.
    #[error("not signed in")]
    NotLoggedIn,
    #[error("already signed in; sign out first")]
    AlreadyLoggedIn,
    #[error("sign-in was cancelled")]
    LoginCancelled,
    #[error("no room `{room_id}`")]
    RoomNotFound { room_id: String },
    /// An argument that is not a valid ID or URL, or a call made twice.
    #[error("{message}")]
    InvalidInput { message: String },
    /// The app's [`crate::SecretBackend`] failed.
    #[error("secret storage: {message}")]
    SecretStorage { message: String },
    /// Anything else, e.g. the server is unreachable or refused the password.
    #[error("{message}")]
    Other { message: String },
}

impl MtrxError {
    pub(crate) fn invalid(message: impl std::fmt::Display) -> Self {
        Self::InvalidInput {
            message: message.to_string(),
        }
    }
}

impl From<mtrx_core::Error> for MtrxError {
    fn from(error: mtrx_core::Error) -> Self {
        use mtrx_core::Error;
        match error {
            Error::NotLoggedIn(_) => Self::NotLoggedIn,
            Error::AlreadyLoggedIn(_) => Self::AlreadyLoggedIn,
            Error::LoginCancelled => Self::LoginCancelled,
            Error::RoomNotFound(room_id) => Self::RoomNotFound { room_id },
            Error::SecretStorage(message) => Self::SecretStorage { message },
            error => Self::Other {
                message: error.to_string(),
            },
        }
    }
}

impl From<std::io::Error> for MtrxError {
    fn from(error: std::io::Error) -> Self {
        Self::Other {
            message: error.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_errors_keep_what_the_apps_match_on() {
        let convert = |e: mtrx_core::Error| MtrxError::from(e);
        assert!(matches!(
            convert(mtrx_core::Error::NotLoggedIn("default".into())),
            MtrxError::NotLoggedIn
        ));
        assert!(matches!(
            convert(mtrx_core::Error::RoomNotFound("!r:example.org".into())),
            MtrxError::RoomNotFound { room_id } if room_id == "!r:example.org"
        ));
        assert!(matches!(
            convert(mtrx_core::Error::SecretStorage("locked".into())),
            MtrxError::SecretStorage { message } if message == "locked"
        ));
        assert!(matches!(
            convert(mtrx_core::Error::Timeline("nothing to send".into())),
            MtrxError::Other { message } if message == "timeline: nothing to send"
        ));
    }
}
