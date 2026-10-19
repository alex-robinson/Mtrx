//! Matrix client logic and state for Mtrx.
//!
//! Frontends (the `mtrx` CLI, the GUI) are built on this crate. It never
//! prints or draws: it exposes plain data types and live update streams.

mod error;
mod profile;
mod session;

pub use error::{Error, Result};
pub use profile::{Profile, SecretStore};
pub use session::Session;
