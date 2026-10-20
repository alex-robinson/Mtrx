//! Matrix client logic and state for Mtrx.
//!
//! Frontends (the `mtrx` CLI, the GUI) are built on this crate. It never
//! prints or draws: it exposes plain data types and live update streams.

// matrix-sdk futures are deeply nested.
#![recursion_limit = "256"]

mod account_data;
mod calls;
mod categories;
mod connection;
mod emoji;
mod encryption;
mod error;
mod google;
mod media;
mod members;
mod notes;
mod notifications;
mod people;
mod permissions;
mod profile;
mod room_colours;
mod room_settings;
mod rooms;
mod scheduled;
mod secrets;
mod send_time;
mod session;
mod timeline;

#[cfg(feature = "test-support")]
pub mod test_support;

pub use calls::{CallService, ServiceKind, recognize_call_link};
pub use categories::{Category, Grouping, Layout, RoomCategories, RoomTag, Section, Sort};
pub use connection::Connection;
pub use encryption::{
    DeviceInfo, RecoveryStatus, RecoveryStep, Verification, VerificationEmoji, VerificationState,
};
pub use error::{Error, Result};
pub use google::{Google, GoogleClient};
pub use media::MediaRef;
pub use members::{Member, Role};
pub use notes::notes_markdown;
pub use notifications::Notification;
pub use people::{Contact, UserInfo};
pub use permissions::{Permissions, SpaceUpdate};
pub use profile::Profile;
pub use room_colours::RoomListColours;
pub use room_settings::{Access, NotifyMode, RoomSettings};
pub use rooms::{NewRoom, RoomKind, RoomList, RoomListState, RoomSummary, UserSummary};
pub use scheduled::{ScheduledFile, ScheduledMessage};
pub use secrets::SecretStore;
pub use send_time::{describe_send_time, parse_send_time};
pub use session::Session;
pub use timeline::{
    Attachment, Event, EventContent, EventKey, Media, MediaKind, MembershipChange, Message,
    MessageKind, PendingSend, Reaction, Reader, Reply, RoomTimeline, SendState, TimelineEntry,
};

pub use url::Url;

/// Matrix identifier types used in this crate's API.
pub use matrix_sdk::ruma::{
    MxcUri, OwnedDeviceId, OwnedEventId, OwnedMxcUri, OwnedRoomId, OwnedRoomOrAliasId, OwnedUserId,
    RoomOrAliasId, UserId,
};

pub(crate) fn system_time(
    ts: matrix_sdk::ruma::MilliSecondsSinceUnixEpoch,
) -> std::time::SystemTime {
    std::time::UNIX_EPOCH + std::time::Duration::from_millis(ts.get().into())
}
