//! Edit text.
use crate::text::highlighter::{self, Highlighter};
use crate::text::{LineHeight, Wrapping};
use crate::{Color, Pixels, Point, Rectangle, Size};

use std::borrow::Cow;
use std::sync::Arc;

/// A component that can be used by widgets to edit multi-line text.
pub trait Editor: Sized + Default {
    /// The font of the [`Editor`].
    type Font: Copy + PartialEq + Default;

    /// Creates a new [`Editor`] laid out with the given text.
    fn with_text(text: &str) -> Self;

    /// Returns true if the [`Editor`] has no contents.
    fn is_empty(&self) -> bool;

    /// Returns the current [`Cursor`] of the [`Editor`].
    fn cursor(&self) -> Cursor;

    /// Returns the current [`Selection`] of the [`Editor`].
    fn selection(&self) -> Selection;

    /// Returns the current selected text of the [`Editor`].
    fn copy(&self) -> Option<String>;

    /// Returns the text of the given line in the [`Editor`], if it exists.
    fn line(&self, index: usize) -> Option<Line<'_>>;

    /// Returns the amount of lines in the [`Editor`].
    fn line_count(&self) -> usize;

    /// Performs an [`Action`] on the [`Editor`].
    fn perform(&mut self, action: Action);

    /// Moves the cursor to the given position.
    fn move_to(&mut self, cursor: Cursor);

    // Mtrx patch (menu actions): for the menus' Undo and Redo.
    /// Returns whether [`Edit::Undo`] would undo something.
    fn can_undo(&self) -> bool;

    /// Returns whether [`Edit::Redo`] would redo something.
    fn can_redo(&self) -> bool;

    // Mtrx patch (editing): for text dragged in the editor.
    /// Returns where a click at the given [`Point`] would put the caret, as
    /// [`Selection::Caret`] has it.
    fn caret_at(&self, point: Point) -> Option<Point>;

    // Mtrx patch (spelling): for the words marked by a highlighter.
    /// Returns the visible runs of text a [`highlighter::Format`] underlines,
    /// each with the color of its line, where [`Selection::Range`] would
    /// have them.
    fn underlines(&self) -> Vec<(Rectangle, Color)>;

    /// Returns the current boundaries of the [`Editor`].
    fn bounds(&self) -> Size;

    /// Returns the minimum boundaries to fit the current contents of
    /// the [`Editor`].
    fn min_bounds(&self) -> Size;

    /// Updates the [`Editor`] with some new attributes.
    fn update(
        &mut self,
        new_bounds: Size,
        new_font: Self::Font,
        new_size: Pixels,
        new_line_height: LineHeight,
        new_wrapping: Wrapping,
        new_highlighter: &mut impl Highlighter,
    );

    /// Runs a text [`Highlighter`] in the [`Editor`].
    fn highlight<H: Highlighter>(
        &mut self,
        font: Self::Font,
        highlighter: &mut H,
        format_highlight: impl Fn(&H::Highlight) -> highlighter::Format<Self::Font>,
    );
}

/// An interaction with an [`Editor`].
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Apply a [`Motion`].
    Move(Motion),
    /// Select text with a given [`Motion`].
    Select(Motion),
    /// Select the word at the current cursor.
    SelectWord,
    /// Select the line at the current cursor.
    SelectLine,
    /// Select the entire buffer.
    SelectAll,
    /// Perform an [`Edit`].
    Edit(Edit),
    /// Click the [`Editor`] at the given [`Point`].
    Click(Point),
    /// Drag the mouse on the [`Editor`] to the given [`Point`].
    Drag(Point),
    /// Scroll the [`Editor`] a certain amount of lines.
    Scroll {
        /// The amount of lines to scroll.
        lines: i32,
    },
}

impl Action {
    /// Returns whether the [`Action`] is an editing action.
    pub fn is_edit(&self) -> bool {
        matches!(self, Self::Edit(_))
    }
}

/// An action that edits text.
#[derive(Debug, Clone, PartialEq)]
pub enum Edit {
    /// Insert the given character.
    Insert(char),
    /// Paste the given text.
    Paste(Arc<String>),
    /// Break the current line.
    Enter,
    /// Indent the current line.
    Indent,
    /// Unindent the current line.
    Unindent,
    /// Delete the previous character.
    Backspace,
    /// Delete the next character.
    Delete,
    // Mtrx patch (surrounding text): for an input method's
    // `DeleteSurrounding`.
    /// Delete the given bytes before and after the selection (or the
    /// cursor), keeping the selection; a line ending counts as its bytes.
    DeleteSurrounding {
        /// The bytes before the selection.
        before: usize,
        /// The bytes after the selection.
        after: usize,
    },
    // Mtrx patch (editing): the editor keeps a history of its edits.
    /// Undo the last step of the history: typing up to a word, a paste, an
    /// input method's commit; the selection returns as it was before.
    Undo,
    /// Redo the last step undone, unless something was edited since.
    Redo,
    /// Delete the selection, or from the cursor to where the [`Motion`]
    /// takes it (a character if it takes it nowhere): ⌥⌫ deletes a word.
    DeleteTo(Motion),
    /// Delete the selection, or to the end of the paragraph (its line
    /// ending, when there), keeping it for [`Edit::Yank`]: ⌃K.
    Kill,
    /// Insert the text last deleted by [`Edit::Kill`], in any editor: ⌃Y.
    Yank,
    /// Swap the characters around the cursor (at the end of a line, the
    /// two before it), and move after them: ⌃T.
    Transpose,
    /// Break the line after the cursor, which stays: ⌃O.
    OpenLine,
    /// Move the selected text to where a click at the [`Point`] would put
    /// the caret (unless that is in it), or copy it there; it stays
    /// selected. Dragging the selection.
    MoveSelection {
        /// Where it is dropped.
        to: Point,
        /// Whether to copy it, keeping it where it was: ⌥ on macOS.
        copy: bool,
    },
}

/// A cursor movement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Motion {
    /// Move left.
    Left,
    /// Move right.
    Right,
    /// Move up.
    Up,
    /// Move down.
    Down,
    /// Move to the left boundary of a word.
    WordLeft,
    /// Move to the right boundary of a word.
    WordRight,
    /// Move to the start of the line.
    Home,
    /// Move to the end of the line.
    End,
    /// Move to the start of the previous window.
    PageUp,
    /// Move to the start of the next window.
    PageDown,
    /// Move to the start of the text.
    DocumentStart,
    /// Move to the end of the text.
    DocumentEnd,
    // Mtrx patch (editing): ⌥↑ and ⌥↓.
    /// Move to the start of the paragraph, or of the one before when
    /// already there.
    ParagraphBackward,
    /// Move to the end of the paragraph, or of the one after when already
    /// there.
    ParagraphForward,
    // Mtrx patch (editing): ⌃A and ⌃E.
    /// Move to the start of the paragraph.
    ParagraphStart,
    /// Move to the end of the paragraph.
    ParagraphEnd,
}

impl Motion {
    /// Widens the [`Motion`], if possible.
    pub fn widen(self) -> Self {
        match self {
            Self::Left => Self::WordLeft,
            Self::Right => Self::WordRight,
            Self::Home => Self::DocumentStart,
            Self::End => Self::DocumentEnd,
            // Mtrx patch (editing): ⌥↑ and ⌥↓ (Ctrl with them elsewhere).
            Self::Up => Self::ParagraphBackward,
            Self::Down => Self::ParagraphForward,
            _ => self,
        }
    }

    /// Returns the [`Direction`] of the [`Motion`].
    pub fn direction(&self) -> Direction {
        match self {
            Self::Left
            | Self::Up
            | Self::WordLeft
            | Self::Home
            | Self::PageUp
            | Self::DocumentStart
            | Self::ParagraphBackward
            | Self::ParagraphStart => Direction::Left,
            Self::Right
            | Self::Down
            | Self::WordRight
            | Self::End
            | Self::PageDown
            | Self::DocumentEnd
            | Self::ParagraphForward
            | Self::ParagraphEnd => Direction::Right,
        }
    }
}

/// A direction in some text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// <-
    Left,
    /// ->
    Right,
}

/// The cursor of an [`Editor`].
#[derive(Debug, Clone)]
pub enum Selection {
    /// Cursor without a selection
    Caret(Point),

    /// Cursor selecting a range of text
    Range(Vec<Rectangle>),
}

/// The range of an [`Editor`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cursor {
    /// The cursor position.
    pub position: Position,

    /// The selection position, if any.
    pub selection: Option<Position>,
}

/// A cursor position in an [`Editor`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    /// The line of text.
    pub line: usize,
    /// The column in the line.
    pub column: usize,
}

/// A line of an [`Editor`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Line<'a> {
    /// The raw text of the [`Line`].
    pub text: Cow<'a, str>,
    /// The line ending of the [`Line`].
    pub ending: LineEnding,
}

/// The line ending of a [`Line`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LineEnding {
    /// Use `\n` for line ending (POSIX-style)
    #[default]
    Lf,
    /// Use `\r\n` for line ending (Windows-style)
    CrLf,
    /// Use `\r` for line ending (many legacy systems)
    Cr,
    /// Use `\n\r` for line ending (some legacy systems)
    LfCr,
    /// No line ending
    None,
}

impl LineEnding {
    /// Gets the string representation of the [`LineEnding`].
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::CrLf => "\r\n",
            Self::Cr => "\r",
            Self::LfCr => "\n\r",
            Self::None => "",
        }
    }
}
