//! The menu bar (macOS): the app menu, Edit, View, Window and Help.
//!
//! The menu takes the shortcuts of its items before the window, so ⌘C,
//! ⌘Z or ⌘F never come as key presses. The Edit and View items send the
//! text system's action messages (`copy:`, `undo:`, ...) to the window's
//! view, which our winit passes on as standard actions
//! (`iced::Event::StandardAction`): the focused field does what the key
//! does, else the app's shortcuts take them as the keys (the timeline's
//! selection, the find bar, the text size). Their items are enabled as the
//! focused field can do them (`InputMethod::enables` in our `iced_core`).
//! AppKit does the rest: Hide, Minimize, Full Screen, and Emoji & Symbols,
//! Start Dictation and AutoFill, which it adds to a menu called Edit. Close
//! hides the window (see `app::closing`). Quit requests to close it, so the
//! session ends first (`applicationShouldTerminate:` in our winit), also
//! from the Dock.

use std::{ffi::CStr, sync::Mutex};

use iced::{
    Subscription,
    futures::{Stream, channel::mpsc},
};
use objc2::{
    MainThreadMarker, MainThreadOnly, define_class,
    rc::Retained,
    runtime::{AnyObject, Sel},
};
use objc2_app_kit::{
    NSApplication, NSEventModifierFlags, NSMenu, NSMenuItem, NSTextFinderAction, NSWindow,
};
use objc2_foundation::{NSObject, NSString};

const WEBSITE: &str = "https://mathesislab.github.io/Mtrx/";

/// A standard action of the window's view, by the action message an item
/// sends: `NSTextFinderAction` in its tag for find.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Standard {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    PasteAsPlainText,
    SelectAll,
    Find,
    FindNext,
    FindPrevious,
    ZoomIn,
    ZoomOut,
    ZoomReset,
}

/// What an item does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Does {
    /// A standard action, which the window's view takes (our winit).
    Standard(Standard),
    /// An action message of AppKit's, to the first responder.
    AppKit(&'static CStr),
    /// Sent to the app, through [`chosen`].
    App(Chosen),
    /// Opens the website.
    Website,
}

/// An item: its title, what it does, and its shortcut (a key with
/// modifiers, ⌘ alone if none are given).
#[derive(Clone, Copy, Debug)]
struct Item {
    title: &'static str,
    does: Does,
    key: &'static str,
    modifiers: NSEventModifierFlags,
}

/// An entry of a menu: an item, a separator, or a submenu with its title.
enum Entry {
    Item(Item),
    Separator,
    Submenu(&'static str, Vec<Entry>),
}

fn item(title: &'static str, does: Does, key: &'static str) -> Entry {
    Entry::Item(Item {
        title,
        does,
        key,
        modifiers: NSEventModifierFlags::Command,
    })
}

fn item_with(
    title: &'static str,
    does: Does,
    key: &'static str,
    modifiers: NSEventModifierFlags,
) -> Entry {
    Entry::Item(Item {
        title,
        does,
        key,
        modifiers,
    })
}

/// The menus, by title (the app menu's is the app's name, which macOS
/// shows in bold).
fn menus() -> Vec<(&'static str, Vec<Entry>)> {
    use Does::{App, AppKit, Standard as S, Website};
    use NSEventModifierFlags as M;
    use Standard::*;

    let shift = M::Command | M::Shift;
    vec![
        (
            "Mtrx",
            vec![
                item("About Mtrx", AppKit(c"orderFrontStandardAboutPanel:"), ""),
                Entry::Separator,
                item("Check for Updates…", App(Chosen::CheckForUpdates), ""),
                item("Settings…", App(Chosen::Settings), ","),
                Entry::Separator,
                Entry::Submenu("Services", Vec::new()),
                Entry::Separator,
                item("Hide Mtrx", AppKit(c"hide:"), "h"),
                item_with(
                    "Hide Others",
                    AppKit(c"hideOtherApplications:"),
                    "h",
                    M::Command | M::Option,
                ),
                item("Show All", AppKit(c"unhideAllApplications:"), ""),
                Entry::Separator,
                item("Quit Mtrx", AppKit(c"terminate:"), "q"),
            ],
        ),
        (
            "Edit",
            vec![
                item("Undo", S(Undo), "z"),
                item_with("Redo", S(Redo), "z", shift),
                Entry::Separator,
                item("Cut", S(Cut), "x"),
                item("Copy", S(Copy), "c"),
                item("Paste", S(Paste), "v"),
                item_with(
                    "Paste as Plain Text",
                    S(PasteAsPlainText),
                    "v",
                    shift | M::Option,
                ),
                item("Select All", S(SelectAll), "a"),
                Entry::Separator,
                Entry::Submenu(
                    "Find",
                    vec![
                        item("Find…", S(Find), "f"),
                        item("Find Next", S(FindNext), "g"),
                        item_with("Find Previous", S(FindPrevious), "g", shift),
                    ],
                ),
            ],
        ),
        (
            "View",
            vec![
                // ⌘+ as macOS apps show it; ⌘= (no ⇧) comes as the key.
                item("Bigger", S(ZoomIn), "+"),
                item("Smaller", S(ZoomOut), "-"),
                item("Actual Size", S(ZoomReset), "0"),
                Entry::Separator,
                item_with(
                    "Enter Full Screen",
                    AppKit(c"toggleFullScreen:"),
                    "f",
                    M::Command | M::Control,
                ),
            ],
        ),
        (
            "Window",
            vec![
                item("Minimize", AppKit(c"performMiniaturize:"), "m"),
                item("Zoom", AppKit(c"performZoom:"), ""),
                Entry::Separator,
                item("Close", AppKit(c"performClose:"), "w"),
                Entry::Separator,
                item("Bring All to Front", AppKit(c"arrangeInFront:"), ""),
            ],
        ),
        ("Help", vec![item("Mtrx Website", Website, "")]),
    ]
}

impl Standard {
    /// The action message and tag the window's view takes it by (see
    /// `vendor/winit`).
    fn message(self) -> (&'static CStr, isize) {
        let find = c"performTextFinderAction:";
        match self {
            Self::Undo => (c"undo:", 0),
            Self::Redo => (c"redo:", 0),
            Self::Cut => (c"cut:", 0),
            Self::Copy => (c"copy:", 0),
            Self::Paste => (c"paste:", 0),
            Self::PasteAsPlainText => (c"pasteAsPlainText:", 0),
            Self::SelectAll => (c"selectAll:", 0),
            Self::Find => (find, NSTextFinderAction::ShowFindInterface.0),
            Self::FindNext => (find, NSTextFinderAction::NextMatch.0),
            Self::FindPrevious => (find, NSTextFinderAction::PreviousMatch.0),
            Self::ZoomIn => (c"makeTextLarger:", 0),
            Self::ZoomOut => (c"makeTextSmaller:", 0),
            Self::ZoomReset => (c"makeTextStandardSize:", 0),
        }
    }
}

define_class!(
    /// The target of the app's own items.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "MtrxMenuTarget"]
    struct Target;

    impl Target {
        #[unsafe(method(showSettings:))]
        fn show_settings(&self, _sender: Option<&AnyObject>) {
            send(Chosen::Settings);
        }

        #[unsafe(method(checkForUpdates:))]
        fn check_for_updates(&self, _sender: Option<&AnyObject>) {
            send(Chosen::CheckForUpdates);
        }

        #[unsafe(method(openWebsite:))]
        fn open_website(&self, _sender: Option<&AnyObject>) {
            let _ = open::that_detached(WEBSITE);
        }
    }
);

/// An item of the app menu that the app acts on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chosen {
    Settings,
    CheckForUpdates,
}

/// Where the app's items send, to [`chosen`].
static CHOSEN: Mutex<Option<mpsc::UnboundedSender<Chosen>>> = Mutex::new(None);

fn send(chosen: Chosen) {
    if let Some(sender) = CHOSEN.lock().unwrap().as_ref() {
        let _ = sender.unbounded_send(chosen);
    }
}

/// The app's items chosen in the app menu.
pub fn chosen() -> Subscription<Chosen> {
    Subscription::run(receive)
}

fn receive() -> impl Stream<Item = Chosen> {
    let (sender, receiver) = mpsc::unbounded();
    *CHOSEN.lock().unwrap() = Some(sender);
    receiver
}

/// The app's menu bar in place of winit's, which has only the app menu.
/// On the main thread, once the app has launched (not in tests).
pub fn install() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    // The target of a menu item is weak: it lives as long as the app.
    let target: Retained<Target> = unsafe { objc2::msg_send![Target::alloc(mtm), init] };
    let bar = NSMenu::new(mtm);
    for (title, entries) in menus() {
        let menu = submenu(mtm, &app, &target, title, entries);
        let item = NSMenuItem::new(mtm);
        item.setSubmenu(Some(&menu));
        bar.addItem(&item);
        match title {
            "Window" => app.setWindowsMenu(Some(&menu)),
            "Help" => app.setHelpMenu(Some(&menu)),
            _ => {}
        }
    }
    app.setMainMenu(Some(&bar));
    std::mem::forget(target);
    // One window, so no tabs (and no Show Tab Bar in View).
    NSWindow::setAllowsAutomaticWindowTabbing(false, mtm);
}

fn submenu(
    mtm: MainThreadMarker,
    app: &NSApplication,
    target: &Target,
    title: &str,
    entries: Vec<Entry>,
) -> Retained<NSMenu> {
    let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(title));
    for entry in entries {
        let item = match entry {
            Entry::Separator => NSMenuItem::separatorItem(mtm),
            Entry::Submenu(title, entries) => {
                let services = title == "Services";
                let submenu = submenu(mtm, app, target, title, entries);
                if services {
                    app.setServicesMenu(Some(&submenu));
                }
                let item = NSMenuItem::new(mtm);
                item.setTitle(&NSString::from_str(title));
                item.setSubmenu(Some(&submenu));
                item
            }
            Entry::Item(item) => menu_item(mtm, target, item),
        };
        menu.addItem(&item);
    }
    menu
}

fn menu_item(mtm: MainThreadMarker, target: &Target, item: Item) -> Retained<NSMenuItem> {
    let (message, tag) = match item.does {
        Does::Standard(standard) => standard.message(),
        Does::AppKit(message) => (message, 0),
        Does::App(Chosen::Settings) => (c"showSettings:", 0),
        Does::App(Chosen::CheckForUpdates) => (c"checkForUpdates:", 0),
        Does::Website => (c"openWebsite:", 0),
    };
    let action = Sel::register(message);
    // SAFETY: each action message takes its sender, as AppKit sends it.
    let menu_item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(item.title),
            Some(action),
            &NSString::from_str(item.key),
        )
    };
    menu_item.setKeyEquivalentModifierMask(item.modifiers);
    menu_item.setTag(tag);
    if matches!(item.does, Does::App(_) | Does::Website) {
        // SAFETY: the target outlives the menu (see `install`).
        unsafe { menu_item.setTarget(Some(target)) };
    }
    menu_item
}

#[cfg(test)]
mod tests {
    use iced::keyboard::{self, Modifiers};

    use super::*;

    /// The standard action winit sends for `standard`.
    fn action(standard: Standard) -> iced_core::event::StandardAction {
        use iced_core::event::StandardAction as A;
        match standard {
            Standard::Undo => A::Undo,
            Standard::Redo => A::Redo,
            Standard::Cut => A::Cut,
            Standard::Copy => A::Copy,
            Standard::Paste => A::Paste,
            Standard::PasteAsPlainText => A::PasteAsPlainText,
            Standard::SelectAll => A::SelectAll,
            Standard::Find => A::Find,
            Standard::FindNext => A::FindNext,
            Standard::FindPrevious => A::FindPrevious,
            Standard::ZoomIn => A::ZoomIn,
            Standard::ZoomOut => A::ZoomOut,
            Standard::ZoomReset => A::ZoomReset,
        }
    }

    fn items(entries: &[Entry]) -> Vec<Item> {
        entries
            .iter()
            .flat_map(|entry| match entry {
                Entry::Item(item) => vec![*item],
                Entry::Submenu(_, entries) => items(entries),
                Entry::Separator => Vec::new(),
            })
            .collect()
    }

    /// Each standard item shows the shortcut its action stands for, which
    /// the field or the app then acts on, and each action has its item.
    #[test]
    fn standard_items_show_their_actions_keys() {
        let all: Vec<Item> = menus()
            .iter()
            .flat_map(|(_, entries)| items(entries))
            .collect();
        let mut actions = Vec::new();
        for item in &all {
            let Does::Standard(standard) = item.does else {
                continue;
            };
            let action = action(standard);
            actions.push(action);
            let keyboard::Event::KeyPressed { key, modifiers, .. } = action.key_press() else {
                panic!("{action:?}: not a key press");
            };
            let mut shown = Modifiers::COMMAND;
            if item.modifiers.contains(NSEventModifierFlags::Shift) {
                shown |= Modifiers::SHIFT;
            }
            if item.modifiers.contains(NSEventModifierFlags::Option) {
                shown |= Modifiers::ALT;
            }
            // ⌘+ is ⇧⌘= on most keyboards; ⌘= zooms in too.
            let key_shown = if item.key == "+" { "=" } else { item.key };
            assert_eq!(
                (key, modifiers),
                (keyboard::Key::Character(key_shown.into()), shown),
                "{}",
                item.title
            );
        }
        for action in iced_core::event::StandardAction::ALL {
            assert!(actions.contains(&action), "{action:?} has no item");
        }
    }
}
