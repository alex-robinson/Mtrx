//! Matrix client logic and state for Mtrx.
//!
//! Frontends (the `mtrx` CLI, the GUI) are built on this crate. It never
//! prints or draws: it exposes plain data types and live update streams.

mod encryption;
mod error;
mod profile;
mod rooms;
mod session;
mod timeline;

pub use encryption::{
    DeviceInfo, RecoveryStatus, Verification, VerificationEmoji, VerificationState,
};
pub use error::{Error, Result};
pub use profile::{Profile, SecretStore};
pub use rooms::{NewRoom, RoomKind, RoomList, RoomListState, RoomSummary};
pub use session::Session;
pub use timeline::{
    Event, EventContent, MembershipChange, Message, MessageKind, PendingSend, RoomTimeline,
    SendState, TimelineEntry,
};

/// Matrix identifier types used in this crate's API.
pub use matrix_sdk::ruma::{
    OwnedDeviceId, OwnedEventId, OwnedRoomId, OwnedRoomOrAliasId, OwnedUserId, RoomOrAliasId,
};

pub(crate) fn system_time(
    ts: matrix_sdk::ruma::MilliSecondsSinceUnixEpoch,
) -> std::time::SystemTime {
    std::time::UNIX_EPOCH + std::time::Duration::from_millis(ts.get().into())
}
