//! Handle events of a user interface.
use crate::input_method;
use crate::keyboard;
use crate::mouse;
use crate::touch;
use crate::window;

/// A user interface event.
///
/// _**Note:** This type is largely incomplete! If you need to track
/// additional events, feel free to [open an issue] and share your use case!_
///
/// [open an issue]: https://github.com/iced-rs/iced/issues
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A keyboard event
    Keyboard(keyboard::Event),

    /// A mouse event
    Mouse(mouse::Event),

    /// A window event
    Window(window::Event),

    /// A touch event
    Touch(touch::Event),

    /// An input method event
    InputMethod(input_method::Event),

    // Mtrx patch (menu actions).
    /// A standard action, such as Undo or Copy, from the platform's menus.
    StandardAction(StandardAction),
}

// Mtrx patch (menu actions): winit's `StandardAction` (our winit). On macOS
// an Edit or View menu takes the shortcuts of its items before the window,
// so these come instead of the key presses.
/// A standard action of the platform's text system and menus: on macOS, an
/// item of the Edit or View menu chosen, or its shortcut pressed.
///
/// A widget does for it what it does for [`StandardAction::key_press`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StandardAction {
    /// Undo the last edit.
    Undo,
    /// Redo the last edit undone.
    Redo,
    /// Cut the selection to the clipboard.
    Cut,
    /// Copy the selection to the clipboard.
    Copy,
    /// Paste the clipboard.
    Paste,
    /// Paste the clipboard as plain text, without its formatting.
    PasteAsPlainText,
    /// Select all.
    SelectAll,
    /// Show the find interface.
    Find,
    /// Go to the next match.
    FindNext,
    /// Go to the previous match.
    FindPrevious,
    /// Make the text bigger.
    ZoomIn,
    /// Make the text smaller.
    ZoomOut,
    /// Make the text its usual size.
    ZoomReset,
}

impl StandardAction {
    /// All of them.
    pub const ALL: [Self; 13] = [
        Self::Undo,
        Self::Redo,
        Self::Cut,
        Self::Copy,
        Self::Paste,
        Self::PasteAsPlainText,
        Self::SelectAll,
        Self::Find,
        Self::FindNext,
        Self::FindPrevious,
        Self::ZoomIn,
        Self::ZoomOut,
        Self::ZoomReset,
    ];

    /// The key press the action stands for, its usual shortcut: ⌘Z for
    /// Undo, ⇧⌘Z for Redo, ⌥⇧⌘V for Paste as Plain Text, ⌘= for Zoom In,
    /// and so on (Ctrl for ⌘ on other platforms). It has no text.
    pub fn key_press(self) -> keyboard::Event {
        use keyboard::key::{Code, Physical};
        use keyboard::{Key, Location, Modifiers};

        let (c, code, modifiers) = match self {
            Self::Undo => ('z', Code::KeyZ, Modifiers::COMMAND),
            Self::Redo => ('z', Code::KeyZ, Modifiers::COMMAND | Modifiers::SHIFT),
            Self::Cut => ('x', Code::KeyX, Modifiers::COMMAND),
            Self::Copy => ('c', Code::KeyC, Modifiers::COMMAND),
            Self::Paste => ('v', Code::KeyV, Modifiers::COMMAND),
            Self::PasteAsPlainText => (
                'v',
                Code::KeyV,
                Modifiers::COMMAND | Modifiers::SHIFT | Modifiers::ALT,
            ),
            Self::SelectAll => ('a', Code::KeyA, Modifiers::COMMAND),
            Self::Find => ('f', Code::KeyF, Modifiers::COMMAND),
            Self::FindNext => ('g', Code::KeyG, Modifiers::COMMAND),
            Self::FindPrevious => ('g', Code::KeyG, Modifiers::COMMAND | Modifiers::SHIFT),
            Self::ZoomIn => ('=', Code::Equal, Modifiers::COMMAND),
            Self::ZoomOut => ('-', Code::Minus, Modifiers::COMMAND),
            Self::ZoomReset => ('0', Code::Digit0, Modifiers::COMMAND),
        };

        let key = Key::Character(c.to_string().into());
        let modified_key = if modifiers.shift() {
            Key::Character(c.to_uppercase().to_string().into())
        } else {
            key.clone()
        };

        keyboard::Event::KeyPressed {
            key,
            modified_key,
            physical_key: Physical::Code(code),
            location: Location::Standard,
            modifiers,
            text: None,
            repeat: false,
        }
    }
}

/// The status of an [`Event`] after being processed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The [`Event`] was **NOT** handled by any widget.
    Ignored,

    /// The [`Event`] was handled and processed by a widget.
    Captured,
}

impl Status {
    /// Merges two [`Status`] into one.
    ///
    /// `Captured` takes precedence over `Ignored`:
    ///
    /// ```
    /// use iced_core::event::Status;
    ///
    /// assert_eq!(Status::Ignored.merge(Status::Ignored), Status::Ignored);
    /// assert_eq!(Status::Ignored.merge(Status::Captured), Status::Captured);
    /// assert_eq!(Status::Captured.merge(Status::Ignored), Status::Captured);
    /// assert_eq!(Status::Captured.merge(Status::Captured), Status::Captured);
    /// ```
    pub fn merge(self, b: Self) -> Self {
        match self {
            Status::Ignored => b,
            Status::Captured => Status::Captured,
        }
    }
}
