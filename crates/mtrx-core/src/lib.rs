//! Matrix client logic and state for Mtrx.
//!
//! Frontends (the `mtrx` CLI, the GUI) are built on this crate. It never
//! prints or draws: it exposes plain data types and live update streams.

mod calls;
mod encryption;
mod error;
mod google;
mod media;
mod permissions;
mod profile;
mod rooms;
mod scheduled;
mod secrets;
mod send_time;
mod session;
mod timeline;

#[cfg(feature = "test-support")]
pub mod test_support;

pub use calls::{CallService, ServiceKind, recognize_call_link};
pub use encryption::{
    DeviceInfo, RecoveryStatus, RecoveryStep, Verification, VerificationEmoji, VerificationState,
};
pub use error::{Error, Result};
pub use google::{Google, GoogleClient};
pub use permissions::{Permissions, SpaceUpdate};
pub use profile::Profile;
pub use rooms::{NewRoom, RoomKind, RoomList, RoomListState, RoomSummary, UserSummary};
pub use scheduled::ScheduledMessage;
pub use secrets::SecretStore;
pub use send_time::{describe_send_time, parse_send_time};
pub use session::Session;
pub use timeline::{
    Event, EventContent, MembershipChange, Message, MessageKind, PendingSend, RoomTimeline,
    SendState, TimelineEntry,
};

pub use url::Url;

/// Matrix identifier types used in this crate's API.
pub use matrix_sdk::ruma::{
    MxcUri, OwnedDeviceId, OwnedEventId, OwnedMxcUri, OwnedRoomId, OwnedRoomOrAliasId, OwnedUserId,
    RoomOrAliasId,
};

pub(crate) fn system_time(
    ts: matrix_sdk::ruma::MilliSecondsSinceUnixEpoch,
) -> std::time::SystemTime {
    std::time::UNIX_EPOCH + std::time::Duration::from_millis(ts.get().into())
}
