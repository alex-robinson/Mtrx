//! The unread count on the app's icon: the Dock on macOS. Other systems
//! show none yet.

use mtrx_core::{RoomKind, RoomSummary};

/// The count shown, to change it only when it changes.
#[derive(Default)]
pub struct Badge {
    shown: u64,
}

impl Badge {
    /// Show `count`; none for 0.
    pub fn show(&mut self, count: u64) {
        if count != self.shown {
            self.shown = count;
            set(count);
        }
    }
}

/// Messages that notify by the account's Matrix notification settings,
/// in all rooms (muted ones have none).
pub fn count(rooms: &[RoomSummary]) -> u64 {
    rooms
        .iter()
        .filter(|r| r.kind != RoomKind::Space)
        .map(|r| r.unread_notifications)
        .sum()
}

#[cfg(target_os = "macos")]
fn set(count: u64) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;
    use objc2_foundation::NSString;

    // AppKit only on the main thread, where iced updates the app (not in
    // tests).
    let Some(main_thread) = MainThreadMarker::new() else {
        return;
    };
    let label = (count > 0).then(|| NSString::from_str(&count.to_string()));
    NSApplication::sharedApplication(main_thread)
        .dockTile()
        .setBadgeLabel(label.as_deref());
}

#[cfg(not(target_os = "macos"))]
fn set(_count: u64) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn room(kind: RoomKind, messages: u64, notifications: u64) -> RoomSummary {
        RoomSummary {
            id: "!room:example.org".try_into().unwrap(),
            name: String::new(),
            avatar: None,
            topic: None,
            kind,
            invited: false,
            encrypted: false,
            public: false,
            unread_messages: messages,
            unread_mentions: 0,
            unread_notifications: notifications,
            last_activity: None,
            parents: Vec::new(),
            tag: mtrx_core::RoomTag::None,
            direct_user: None,
            alias: None,
            joined_members: 2,
        }
    }

    /// Messages that notify count, in rooms and direct chats; other unread
    /// messages and spaces do not.
    #[test]
    fn counts_notifying_messages() {
        assert_eq!(count(&[]), 0);
        let rooms = [
            room(RoomKind::Room, 12, 2),
            room(RoomKind::Direct, 3, 3),
            room(RoomKind::Room, 40, 0),
            room(RoomKind::Space, 5, 5),
        ];
        assert_eq!(count(&rooms), 5);
    }
}
