//! Matrix client logic and state for Mtrx.
//!
//! Frontends (the `mtrx` CLI, the GUI) are built on this crate. It never
//! prints or draws: it exposes plain data types and live update streams.

mod error;
mod profile;
mod rooms;
mod session;

pub use error::{Error, Result};
pub use profile::{Profile, SecretStore};
pub use rooms::{NewRoom, RoomKind, RoomList, RoomListState, RoomSummary};
pub use session::Session;

/// Matrix identifier types used in this crate's API.
pub use matrix_sdk::ruma::{
    OwnedDeviceId, OwnedEventId, OwnedRoomId, OwnedRoomOrAliasId, OwnedUserId, RoomOrAliasId,
};
