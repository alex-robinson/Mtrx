//! Mtrx patch (editing): the history of an [`Editor`]'s edits, for undo
//! and redo.
//!
//! Edits are grouped into steps as a text view does: typing at the caret
//! up to a word (the step ends before the first letter after a space that
//! followed a word), and the same for deleting with Backspace or Delete. A
//! caret move or a selection change, switching between typing and
//! deleting, and a paste each start a new step; an input method's
//! deletion around the caret joins the commit that follows it.
//!
//! [`Editor`]: super::Editor
use crate::core::text::editor::Edit;

use cosmic_text::Edit as _;

use std::collections::VecDeque;

/// The steps kept; the oldest go first.
const LIMIT: usize = 300;

/// The undo and redo steps of an editor.
#[derive(Debug, Default)]
pub(super) struct History {
    undo: VecDeque<Step>,
    redo: Vec<Step>,
    /// How the last step may grow, while it may.
    open: Option<Open>,
}

/// Edits undone at once.
#[derive(Debug)]
struct Step {
    change: cosmic_text::Change,
    before: Caret,
    after: Caret,
}

/// The cursor and the selection.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Caret {
    cursor: cosmic_text::Cursor,
    selection: cosmic_text::Selection,
}

impl Caret {
    fn of(editor: &cosmic_text::Editor<'static>) -> Self {
        Self {
            cursor: editor.cursor(),
            selection: editor.selection(),
        }
    }

    fn restore(self, editor: &mut cosmic_text::Editor<'static>) {
        editor.set_cursor(self.cursor);
        editor.set_selection(self.selection);
    }
}

/// What an edit is, for grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// A character typed (a line break too).
    Typing(char),
    /// The character before the caret deleted.
    Backspace(char),
    /// The character after the caret deleted.
    Delete(char),
    /// An input method's deletion around the caret, before its commit.
    Surrounding,
    /// Text pasted, or committed by an input method.
    Paste,
    /// Anything else: a step of its own.
    Alone,
}

impl Kind {
    fn of(edit: &Edit, editor: &cosmic_text::Editor<'static>) -> Self {
        let cursor = editor.cursor();
        let line = |index: usize| {
            super::buffer_from_editor(editor)
                .lines
                .get(index)
                .map(cosmic_text::BufferLine::text)
        };
        let text = line(cursor.line).unwrap_or_default();

        match edit {
            Edit::Insert(c) => Self::Typing(*c),
            Edit::Enter => Self::Typing('\n'),
            // A line ending counts as a space.
            Edit::Backspace => Self::Backspace(
                text.get(..cursor.index)
                    .and_then(|before| before.chars().next_back())
                    .unwrap_or('\n'),
            ),
            Edit::Delete => Self::Delete(
                text.get(cursor.index..)
                    .and_then(|after| after.chars().next())
                    .unwrap_or('\n'),
            ),
            Edit::DeleteSurrounding { .. } => Self::Surrounding,
            Edit::Paste(_) => Self::Paste,
            Edit::Indent | Edit::Unindent | Edit::Undo | Edit::Redo => {
                Self::Alone
            }
        }
    }

    /// The character typed or deleted.
    fn character(self) -> Option<char> {
        match self {
            Self::Typing(c) | Self::Backspace(c) | Self::Delete(c) => Some(c),
            Self::Surrounding | Self::Paste | Self::Alone => None,
        }
    }

    /// Whether an edit of this kind may join a step of `kind`.
    fn joins(self, kind: Self) -> bool {
        match (self, kind) {
            (Self::Typing(_), Self::Typing(_))
            | (Self::Backspace(_), Self::Backspace(_))
            | (Self::Delete(_), Self::Delete(_))
            | (Self::Paste, Self::Surrounding) => true,
            _ => false,
        }
    }
}

/// How the last step may grow.
#[derive(Debug, Clone, Copy)]
struct Open {
    kind: Kind,
    word: Word,
}

/// Where typing (or deleting) is in its words: a step ends before the
/// first letter after a space that followed a word.
#[derive(Debug, Clone, Copy, Default)]
struct Word {
    /// A letter was typed.
    letter: bool,
    /// Then a space.
    space: bool,
}

impl Word {
    fn breaks_before(self, c: char) -> bool {
        self.space && !c.is_whitespace()
    }

    fn then(self, c: char) -> Self {
        if c.is_whitespace() {
            Self {
                space: self.letter,
                ..self
            }
        } else {
            Self {
                letter: true,
                space: false,
            }
        }
    }
}

/// An edit being made, as [`History::start`] found it.
#[derive(Debug)]
pub(super) struct Pending {
    kind: Kind,
    before: Caret,
    selected: bool,
}

impl History {
    /// Starts recording `edit`, about to be made to `editor`.
    pub fn start(
        &mut self,
        edit: &Edit,
        editor: &mut cosmic_text::Editor<'static>,
    ) -> Pending {
        let pending = Pending {
            kind: Kind::of(edit, editor),
            before: Caret::of(editor),
            selected: editor.selection_bounds().is_some(),
        };

        editor.start_change();

        pending
    }

    /// Records the edit started as `pending`, now made to `editor`: into
    /// the last step if it continues it, else as a new one.
    pub fn record(
        &mut self,
        pending: Pending,
        editor: &mut cosmic_text::Editor<'static>,
    ) {
        let Some(change) = editor
            .finish_change()
            .filter(|change| !change.items.is_empty())
        else {
            return;
        };

        let after = Caret::of(editor);
        let kind = pending.kind;

        let joining = self.open.filter(|open| {
            kind.joins(open.kind)
                && !pending.selected
                && !kind.character().is_some_and(|c| open.word.breaks_before(c))
                && self
                    .undo
                    .back()
                    .is_some_and(|last| last.after == pending.before)
        });

        self.redo.clear();

        match (joining, self.undo.back_mut()) {
            (Some(_), Some(last)) => {
                last.change.items.extend(change.items);
                last.after = after;
            }
            _ => {
                self.undo.push_back(Step {
                    change,
                    before: pending.before,
                    after,
                });

                if self.undo.len() > LIMIT {
                    let _ = self.undo.pop_front();
                }
            }
        }

        let word = joining.map(|open| open.word).unwrap_or_default();

        self.open = match kind {
            // A selection deleted (or cut) is a step of its own; typed
            // over, the typing goes on in its step.
            Kind::Backspace(_) | Kind::Delete(_) if pending.selected => None,
            Kind::Typing(c) | Kind::Backspace(c) | Kind::Delete(c) => {
                Some(Open {
                    kind,
                    word: word.then(c),
                })
            }
            Kind::Surrounding => Some(Open {
                kind,
                word: Word::default(),
            }),
            Kind::Paste | Kind::Alone => None,
        };
    }

    /// Ends the last step: the caret moved, or the selection changed.
    pub fn close(&mut self) {
        self.open = None;
    }

    /// Undoes the last step in `editor`; returns the topmost line changed.
    pub fn undo(
        &mut self,
        editor: &mut cosmic_text::Editor<'static>,
    ) -> Option<usize> {
        self.open = None;

        let step = self.undo.pop_back()?;
        let mut change = step.change.clone();
        change.reverse();

        let _ = editor.apply_change(&change);
        step.before.restore(editor);

        let line = topmost(&change);
        self.redo.push(step);

        Some(line)
    }

    /// Redoes the last step undone in `editor`; returns the topmost line
    /// changed.
    pub fn redo(
        &mut self,
        editor: &mut cosmic_text::Editor<'static>,
    ) -> Option<usize> {
        self.open = None;

        let step = self.redo.pop()?;

        let _ = editor.apply_change(&step.change);
        step.after.restore(editor);

        let line = topmost(&step.change);
        self.undo.push_back(step);

        Some(line)
    }
}

fn topmost(change: &cosmic_text::Change) -> usize {
    change
        .items
        .iter()
        .map(|item| item.start.line.min(item.end.line))
        .min()
        .unwrap_or_default()
}
