//! Listen to input method events.
use crate::event::StandardAction;
use crate::{Pixels, Rectangle};

use std::ops::Range;

/// The input method strategy of a widget.
#[derive(Debug, Clone, PartialEq)]
pub enum InputMethod<T = String> {
    /// Input method is disabled.
    Disabled,
    /// Input method is enabled.
    Enabled {
        /// The area of the cursor of the input method.
        ///
        /// This area should not be covered.
        cursor: Rectangle,
        /// The [`Purpose`] of the input method.
        purpose: Purpose,
        /// The preedit to overlay on top of the input method dialog, if needed.
        ///
        /// Ideally, your widget will show pre-edits on-the-spot; but, since that can
        /// be tricky, you can instead provide the current pre-edit here and the
        /// runtime will display it as an overlay (i.e. "Over-the-spot IME").
        preedit: Option<Preedit<T>>,
        // Mtrx patch (surrounding text): see [`SurroundingText`].
        /// The text around the cursor, if the widget can tell, so the input
        /// method can replace what was typed before the cursor.
        surrounding_text: Option<SurroundingText>,
        // Mtrx patch (menu actions).
        /// What the widget's editing can do now, for the menus.
        edit: EditState,
    },
}

// Mtrx patch (menu actions): for the items of an Edit menu, which macOS
// enables as the focused text view can do them.
/// What the editing of the widget with an [`InputMethod`] can do now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct EditState {
    /// It has something to undo.
    pub can_undo: bool,
    /// It has something to redo.
    pub can_redo: bool,
    /// Text is selected in it.
    pub has_selection: bool,
}

// Mtrx patch (surrounding text): winit 0.31's `ImeSurroundingText`, which
// iced does not have yet. macOS replaces a letter held for its accents
// with the accent picked, by a range in the text it is told about.
/// The text around the cursor of an [`InputMethod`]: an excerpt of the
/// widget's text, without the preedit.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SurroundingText {
    /// The excerpt, shorter than [`Self::MAX_BYTES`].
    pub text: String,
    /// The position of the cursor, in bytes from the start of `text`.
    pub cursor: usize,
    /// The other end of the selection, in bytes; the cursor if none.
    pub anchor: usize,
}

impl SurroundingText {
    /// The longest excerpt: winit takes fewer than 4000 bytes.
    pub const MAX_BYTES: usize = 3999;

    /// The excerpt of `text` around the selection from `anchor` to
    /// `cursor` (byte offsets on character boundaries), cut to
    /// [`Self::MAX_BYTES`] around the selection; `None` if the selection
    /// alone is longer.
    pub fn around(text: &str, cursor: usize, anchor: usize) -> Option<Self> {
        debug_assert!(text.is_char_boundary(cursor));
        debug_assert!(text.is_char_boundary(anchor));

        let (low, high) = (cursor.min(anchor), cursor.max(anchor));
        if high - low > Self::MAX_BYTES {
            return None;
        }

        // As much on either side of the selection.
        let spare = Self::MAX_BYTES - (high - low);
        let mut start = low.saturating_sub(spare / 2);
        let mut end = (start + Self::MAX_BYTES).min(text.len());
        start = start.min(end.saturating_sub(Self::MAX_BYTES));

        // Inwards to whole characters; never past the selection, whose
        // ends are on character boundaries.
        while !text.is_char_boundary(start) {
            start += 1;
        }
        while !text.is_char_boundary(end) {
            end -= 1;
        }

        Some(Self {
            text: text[start..end].to_owned(),
            cursor: cursor - start,
            anchor: anchor - start,
        })
    }
}

/// The pre-edit of an [`InputMethod`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Preedit<T = String> {
    /// The current content.
    pub content: T,
    /// The selected range of the content.
    pub selection: Option<Range<usize>>,
    /// The text size of the content.
    pub text_size: Option<Pixels>,
}

impl<T> Preedit<T> {
    /// Creates a new empty [`Preedit`].
    pub fn new() -> Self
    where
        T: Default,
    {
        Self::default()
    }

    /// Turns a [`Preedit`] into its owned version.
    pub fn to_owned(&self) -> Preedit
    where
        T: AsRef<str>,
    {
        Preedit {
            content: self.content.as_ref().to_owned(),
            selection: self.selection.clone(),
            text_size: self.text_size,
        }
    }
}

impl Preedit {
    /// Borrows the contents of a [`Preedit`].
    pub fn as_ref(&self) -> Preedit<&str> {
        Preedit {
            content: &self.content,
            selection: self.selection.clone(),
            text_size: self.text_size,
        }
    }
}

/// The purpose of an [`InputMethod`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Purpose {
    /// No special hints for the IME (default).
    #[default]
    Normal,
    /// The IME is used for secure input (e.g. passwords).
    Secure,
    /// The IME is used to input into a terminal.
    ///
    /// For example, that could alter OSK on Wayland to show extra buttons.
    Terminal,
}

impl InputMethod {
    /// Merges two [`InputMethod`] strategies, prioritizing the first one when both open:
    /// ```
    /// # use iced_core::input_method::{InputMethod, Purpose, Preedit};
    /// # use iced_core::{Point, Rectangle, Size};
    ///
    /// let open = InputMethod::Enabled {
    ///     cursor: Rectangle::new(Point::ORIGIN, Size::UNIT),
    ///     purpose: Purpose::Normal,
    ///     preedit: Some(Preedit { content: "1".to_owned(), selection: None, text_size: None }),
    ///     surrounding_text: None,
    ///     edit: Default::default(),
    /// };
    ///
    /// let open_2 = InputMethod::Enabled {
    ///     cursor: Rectangle::new(Point::ORIGIN, Size::UNIT),
    ///     purpose: Purpose::Secure,
    ///     preedit: Some(Preedit { content: "2".to_owned(), selection: None, text_size: None }),
    ///     surrounding_text: None,
    ///     edit: Default::default(),
    /// };
    ///
    /// let mut ime = InputMethod::Disabled;
    ///
    /// ime.merge(&open);
    /// assert_eq!(ime, open);
    ///
    /// ime.merge(&open_2);
    /// assert_eq!(ime, open);
    /// ```
    pub fn merge<T: AsRef<str>>(&mut self, other: &InputMethod<T>) {
        if let InputMethod::Enabled { .. } = self {
            return;
        }

        *self = other.to_owned();
    }

    /// Returns true if the [`InputMethod`] is open.
    pub fn is_enabled(&self) -> bool {
        matches!(self, Self::Enabled { .. })
    }

    // Mtrx patch (menu actions).
    /// Whether `action` can do something now, to enable its menu item: the
    /// editing ones as the focused widget can do them (its [`EditState`]);
    /// with none focused, those an app may take for something else: Copy
    /// and Select All (a selection of its own), Find and the text size.
    ///
    /// On macOS a disabled item still takes its shortcut, which then
    /// reaches nothing: this errs on the side of enabled.
    pub fn enables(&self, action: StandardAction) -> bool {
        match self {
            Self::Disabled => !matches!(
                action,
                StandardAction::Undo
                    | StandardAction::Redo
                    | StandardAction::Cut
                    | StandardAction::Paste
                    | StandardAction::PasteAsPlainText
            ),
            Self::Enabled { purpose, edit, .. } => match action {
                StandardAction::Undo => edit.can_undo,
                StandardAction::Redo => edit.can_redo,
                StandardAction::Cut => {
                    edit.has_selection && *purpose != Purpose::Secure
                }
                _ => true,
            },
        }
    }
}

impl<T> InputMethod<T> {
    /// Turns an [`InputMethod`] into its owned version.
    pub fn to_owned(&self) -> InputMethod
    where
        T: AsRef<str>,
    {
        match self {
            Self::Disabled => InputMethod::Disabled,
            Self::Enabled {
                cursor,
                purpose,
                preedit,
                surrounding_text,
                edit,
            } => InputMethod::Enabled {
                cursor: *cursor,
                purpose: *purpose,
                preedit: preedit.as_ref().map(Preedit::to_owned),
                surrounding_text: surrounding_text.clone(),
                edit: *edit,
            },
        }
    }
}

/// Describes [input method](https://en.wikipedia.org/wiki/Input_method) events.
///
/// This is also called a "composition event".
///
/// Most keypresses using a latin-like keyboard layout simply generate a
/// [`keyboard::Event::KeyPressed`](crate::keyboard::Event::KeyPressed).
/// However, one couldn't possibly have a key for every single
/// unicode character that the user might want to type. The solution operating systems employ is
/// to allow the user to type these using _a sequence of keypresses_ instead.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Event {
    /// Notifies when the IME was opened.
    ///
    /// After getting this event you could receive [`Preedit`][Self::Preedit] and
    /// [`Commit`][Self::Commit] events. You should also start performing IME related requests
    /// like [`Shell::request_input_method`].
    ///
    /// [`Shell::request_input_method`]: crate::Shell::request_input_method
    Opened,

    /// Notifies when a new composing text should be set at the cursor position.
    ///
    /// The value represents a pair of the preedit string and the cursor begin position and end
    /// position. When it's `None`, the cursor should be hidden. When `String` is an empty string
    /// this indicates that preedit was cleared.
    ///
    /// The cursor range is byte-wise indexed.
    Preedit(String, Option<Range<usize>>),

    /// Notifies when text should be inserted into the editor widget.
    ///
    /// Right before this event, an empty [`Self::Preedit`] event will be issued.
    Commit(String),

    // Mtrx patch (surrounding text): winit 0.31's `Ime::DeleteSurrounding`.
    /// Notifies when the text around the cursor or selection should be
    /// deleted, before a [`Commit`][Self::Commit] or
    /// [`Preedit`][Self::Preedit] that replaces it.
    ///
    /// Counted in bytes of the [`SurroundingText`], outwards from the
    /// selection, which stays (the commit replaces it).
    DeleteSurrounding {
        /// The bytes to delete before the selection.
        before_bytes: usize,
        /// The bytes to delete after the selection.
        after_bytes: usize,
    },

    /// Notifies when the IME was disabled.
    ///
    /// After receiving this event you won't get any more [`Preedit`][Self::Preedit] or
    /// [`Commit`][Self::Commit] events until the next [`Opened`][Self::Opened] event. You should
    /// also stop issuing IME related requests like [`Shell::request_input_method`] and clear
    /// pending preedit text.
    ///
    /// [`Shell::request_input_method`]: crate::Shell::request_input_method
    Closed,
}
