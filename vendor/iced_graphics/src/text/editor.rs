//! Draw and edit text.
// Mtrx patch (editing).
mod history;

use crate::core::text::editor::{
    self, Action, Cursor, Direction, Edit, Motion, Position, Selection,
};
use crate::core::text::highlighter::{self, Highlighter};
use crate::core::text::{LineHeight, Wrapping};
use crate::core::{Color, Font, Pixels, Point, Rectangle, Size};
use crate::text;

use cosmic_text::Edit as _;

use std::borrow::Cow;
use std::fmt;
use std::sync::{self, Arc, RwLock};

/// A multi-line text editor.
#[derive(Debug, PartialEq)]
pub struct Editor(Option<Arc<Internal>>);

struct Internal {
    editor: cosmic_text::Editor<'static>,
    selection: RwLock<Option<Selection>>,
    font: Font,
    bounds: Size,
    topmost_line_changed: Option<usize>,
    version: text::Version,
    // Mtrx patch (editing).
    history: history::History,
    // Mtrx patch (spelling): the colors of the underlines, by the glyph
    // metadata `underline_metadata` gives them.
    underline_colors: Vec<Color>,
}

impl Editor {
    /// Creates a new empty [`Editor`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the buffer of the [`Editor`].
    pub fn buffer(&self) -> &cosmic_text::Buffer {
        buffer_from_editor(&self.internal().editor)
    }

    /// Creates a [`Weak`] reference to the [`Editor`].
    ///
    /// This is useful to avoid cloning the [`Editor`] when
    /// referential guarantees are unnecessary. For instance,
    /// when creating a rendering tree.
    pub fn downgrade(&self) -> Weak {
        let editor = self.internal();

        Weak {
            raw: Arc::downgrade(editor),
            bounds: editor.bounds,
        }
    }

    fn internal(&self) -> &Arc<Internal> {
        self.0
            .as_ref()
            .expect("Editor should always be initialized")
    }

    fn with_internal_mut<T>(
        &mut self,
        f: impl FnOnce(&mut Internal) -> T,
    ) -> T {
        let editor =
            self.0.take().expect("Editor should always be initialized");

        // TODO: Handle multiple strong references somehow
        let mut internal = Arc::try_unwrap(editor)
            .expect("Editor cannot have multiple strong references");

        // Clear cursor cache
        let _ = internal
            .selection
            .write()
            .expect("Write to cursor cache")
            .take();

        let result = f(&mut internal);

        self.0 = Some(Arc::new(internal));

        result
    }
}

impl editor::Editor for Editor {
    type Font = Font;

    fn with_text(text: &str) -> Self {
        let mut buffer = cosmic_text::Buffer::new_empty(cosmic_text::Metrics {
            font_size: 1.0,
            line_height: 1.0,
        });

        let mut font_system =
            text::font_system().write().expect("Write font system");

        buffer.set_text(
            font_system.raw(),
            text,
            &cosmic_text::Attrs::new(),
            cosmic_text::Shaping::Advanced,
            None,
        );

        Editor(Some(Arc::new(Internal {
            editor: cosmic_text::Editor::new(buffer),
            version: font_system.version(),
            ..Default::default()
        })))
    }

    fn is_empty(&self) -> bool {
        let buffer = self.buffer();

        buffer.lines.is_empty()
            || (buffer.lines.len() == 1 && buffer.lines[0].text().is_empty())
    }

    fn line(&self, index: usize) -> Option<editor::Line<'_>> {
        self.buffer().lines.get(index).map(|line| editor::Line {
            text: Cow::Borrowed(line.text()),
            ending: match line.ending() {
                cosmic_text::LineEnding::Lf => editor::LineEnding::Lf,
                cosmic_text::LineEnding::CrLf => editor::LineEnding::CrLf,
                cosmic_text::LineEnding::Cr => editor::LineEnding::Cr,
                cosmic_text::LineEnding::LfCr => editor::LineEnding::LfCr,
                cosmic_text::LineEnding::None => editor::LineEnding::None,
            },
        })
    }

    fn line_count(&self) -> usize {
        self.buffer().lines.len()
    }

    fn copy(&self) -> Option<String> {
        self.internal().editor.copy_selection()
    }

    fn selection(&self) -> editor::Selection {
        let internal = self.internal();

        if let Ok(Some(cursor)) = internal.selection.read().as_deref() {
            return cursor.clone();
        }

        let cursor = internal.editor.cursor();
        let buffer = buffer_from_editor(&internal.editor);

        let cursor = match internal.editor.selection_bounds() {
            Some((start, end)) => {
                let line_height = buffer.metrics().line_height;
                let selected_lines = end.line - start.line + 1;

                let visual_lines_offset =
                    visual_lines_offset(start.line, buffer);

                let regions = buffer
                    .lines
                    .iter()
                    .skip(start.line)
                    .take(selected_lines)
                    .enumerate()
                    .flat_map(|(i, line)| {
                        highlight_line(
                            line,
                            if i == 0 { start.index } else { 0 },
                            if i == selected_lines - 1 {
                                end.index
                            } else {
                                line.text().len()
                            },
                        )
                    })
                    .enumerate()
                    .filter_map(|(visual_line, (x, width))| {
                        if width > 0.0 {
                            Some(Rectangle {
                                x,
                                width,
                                y: (visual_line as i32 + visual_lines_offset)
                                    as f32
                                    * line_height
                                    - buffer.scroll().vertical,
                                height: line_height,
                            })
                        } else {
                            None
                        }
                    })
                    .collect();

                Selection::Range(regions)
            }
            // Mtrx patch (editing): out of here, for `caret_at` too.
            _ => Selection::Caret(caret(buffer, cursor)),
        };

        *internal.selection.write().expect("Write to cursor cache") =
            Some(cursor.clone());

        cursor
    }

    fn cursor(&self) -> Cursor {
        let editor = &self.internal().editor;

        let position = {
            let cursor = editor.cursor();

            Position {
                line: cursor.line,
                column: cursor.index,
            }
        };

        let selection = match editor.selection() {
            cosmic_text::Selection::None => None,
            cosmic_text::Selection::Normal(cursor)
            | cosmic_text::Selection::Line(cursor)
            | cosmic_text::Selection::Word(cursor) => Some(Position {
                line: cursor.line,
                column: cursor.index,
            }),
        };

        Cursor {
            position,
            selection,
        }
    }

    fn perform(&mut self, action: Action) {
        let mut font_system =
            text::font_system().write().expect("Write font system");

        self.with_internal_mut(|internal| {
            // Mtrx patch (editing): a move, a click or a selection ends the
            // step of the history being typed.
            if !matches!(action, Action::Edit(_) | Action::Scroll { .. }) {
                internal.history.close();
            }

            let editor = &mut internal.editor;

            match action {
                // Motion events
                Action::Move(motion) => {
                    if let Some((start, end)) = editor.selection_bounds() {
                        editor.set_selection(cosmic_text::Selection::None);

                        match motion {
                            // These motions are performed as-is even when a selection
                            // is present
                            Motion::Home
                            | Motion::End
                            | Motion::DocumentStart
                            | Motion::DocumentEnd => {
                                perform_motion(
                                    editor,
                                    &mut font_system,
                                    motion,
                                );
                            }
                            // Mtrx patch (editing): to the edge of the
                            // paragraph where the selection starts (ends),
                            // as a text view does.
                            Motion::ParagraphBackward
                            | Motion::ParagraphStart => {
                                editor.set_cursor(start);
                                perform_motion(
                                    editor,
                                    &mut font_system,
                                    Motion::ParagraphStart,
                                );
                            }
                            Motion::ParagraphForward | Motion::ParagraphEnd => {
                                editor.set_cursor(end);
                                perform_motion(
                                    editor,
                                    &mut font_system,
                                    Motion::ParagraphEnd,
                                );
                            }
                            // Other motions simply move the cursor to one end of the selection
                            _ => editor.set_cursor(match motion.direction() {
                                Direction::Left => start,
                                Direction::Right => end,
                            }),
                        }
                    } else {
                        // Mtrx patch (editing).
                        perform_motion(editor, &mut font_system, motion);
                    }
                }

                // Selection events
                Action::Select(motion) => {
                    let cursor = editor.cursor();

                    if editor.selection_bounds().is_none() {
                        editor.set_selection(cosmic_text::Selection::Normal(
                            cursor,
                        ));
                    }

                    // Mtrx patch (editing).
                    perform_motion(editor, &mut font_system, motion);

                    // Deselect if selection matches cursor position
                    if let Some((start, end)) = editor.selection_bounds()
                        && start.line == end.line
                        && start.index == end.index
                    {
                        editor.set_selection(cosmic_text::Selection::None);
                    }
                }
                Action::SelectWord => {
                    let cursor = editor.cursor();

                    editor.set_selection(cosmic_text::Selection::Word(cursor));
                }
                Action::SelectLine => {
                    let cursor = editor.cursor();

                    editor.set_selection(cosmic_text::Selection::Line(cursor));
                }
                Action::SelectAll => {
                    let buffer = buffer_from_editor(editor);

                    if buffer.lines.len() > 1
                        || buffer
                            .lines
                            .first()
                            .is_some_and(|line| !line.text().is_empty())
                    {
                        let cursor = editor.cursor();

                        editor.set_selection(cosmic_text::Selection::Normal(
                            cosmic_text::Cursor {
                                line: 0,
                                index: 0,
                                ..cursor
                            },
                        ));

                        editor.action(
                            font_system.raw(),
                            cosmic_text::Action::Motion(
                                cosmic_text::Motion::BufferEnd,
                            ),
                        );
                    }
                }

                // Editing events
                Action::Edit(edit) => {
                    let topmost_line_before_edit = editor
                        .selection_bounds()
                        .map(|(start, _)| start)
                        .unwrap_or_else(|| editor.cursor())
                        .line;

                    // Mtrx patch (editing): undo and redo replay the
                    // history, other edits are recorded in it.
                    let replayed = match edit {
                        Edit::Undo => internal.history.undo(editor),
                        Edit::Redo => internal.history.redo(editor),
                        edit => {
                            let pending = internal.history.start(&edit, editor);
                            perform_edit(editor, &mut font_system, edit);
                            internal.history.record(pending, editor);

                            None
                        }
                    };

                    let cursor = editor.cursor();
                    let selection_start = editor
                        .selection_bounds()
                        .map(|(start, _)| start)
                        .unwrap_or(cursor);

                    internal.topmost_line_changed = Some(
                        selection_start
                            .line
                            .min(topmost_line_before_edit)
                            .min(replayed.unwrap_or(usize::MAX)),
                    );
                }

                // Mouse events
                Action::Click(position) => {
                    editor.action(
                        font_system.raw(),
                        cosmic_text::Action::Click {
                            x: position.x as i32,
                            y: position.y as i32,
                        },
                    );
                }
                Action::Drag(position) => {
                    editor.action(
                        font_system.raw(),
                        cosmic_text::Action::Drag {
                            x: position.x as i32,
                            y: position.y as i32,
                        },
                    );

                    // Deselect if selection matches cursor position
                    if let Some((start, end)) = editor.selection_bounds()
                        && start.line == end.line
                        && start.index == end.index
                    {
                        editor.set_selection(cosmic_text::Selection::None);
                    }
                }
                Action::Scroll { lines } => {
                    editor.action(
                        font_system.raw(),
                        cosmic_text::Action::Scroll {
                            pixels: lines as f32
                                * buffer_from_editor(editor)
                                    .metrics()
                                    .line_height,
                        },
                    );
                }
            }
        });
    }

    fn move_to(&mut self, cursor: Cursor) {
        self.with_internal_mut(|internal| {
            // Mtrx patch (editing).
            internal.history.close();

            // TODO: Expose `Affinity`
            internal.editor.set_cursor(cosmic_text::Cursor {
                line: cursor.position.line,
                index: cursor.position.column,
                affinity: cosmic_text::Affinity::Before,
            });

            if let Some(selection) = cursor.selection {
                internal
                    .editor
                    .set_selection(cosmic_text::Selection::Normal(
                        cosmic_text::Cursor {
                            line: selection.line,
                            index: selection.column,
                            affinity: cosmic_text::Affinity::Before,
                        },
                    ));
            }
        });
    }

    // Mtrx patch (menu actions).
    fn can_undo(&self) -> bool {
        self.internal().history.can_undo()
    }

    fn can_redo(&self) -> bool {
        self.internal().history.can_redo()
    }

    // Mtrx patch (editing).
    fn caret_at(&self, point: Point) -> Option<Point> {
        let buffer = self.buffer();

        buffer
            .hit(point.x, point.y)
            .map(|cursor| caret(buffer, cursor))
    }

    // Mtrx patch (spelling).
    fn underlines(&self) -> Vec<(Rectangle, Color)> {
        let internal = self.internal();
        let buffer = buffer_from_editor(&internal.editor);
        let mut underlines = Vec::new();

        for run in buffer.layout_runs() {
            let mut glyphs = run.glyphs.iter().peekable();

            while let Some(first) = glyphs.next() {
                let Some(color) =
                    underline_color(&internal.underline_colors, first.metadata)
                else {
                    continue;
                };
                let mut end = first.x + first.w;

                while let Some(next) =
                    glyphs.next_if(|glyph| glyph.metadata == first.metadata)
                {
                    end = next.x + next.w;
                }

                underlines.push((
                    Rectangle {
                        x: first.x,
                        y: run.line_y,
                        width: end - first.x,
                        height: run.line_top + run.line_height - run.line_y,
                    },
                    color,
                ));
            }
        }

        underlines
    }

    fn bounds(&self) -> Size {
        self.internal().bounds
    }

    fn min_bounds(&self) -> Size {
        let internal = self.internal();
        let buffer = buffer_from_editor(&internal.editor);

        // The whole content, not only the lines scrolled into view: those
        // count differently by whether they are empty, so a measure of
        // them changes while scrolling (all lines are laid out in `update`).
        let line_height = buffer.metrics().line_height;

        buffer
            .lines
            .iter()
            .filter_map(cosmic_text::BufferLine::layout_opt)
            .flatten()
            .fold(Size::ZERO, |bounds, line| {
                Size::new(
                    bounds.width.max(line.w),
                    bounds.height + line.line_height_opt.unwrap_or(line_height),
                )
            })
    }

    fn update(
        &mut self,
        new_bounds: Size,
        new_font: Font,
        new_size: Pixels,
        new_line_height: LineHeight,
        new_wrapping: Wrapping,
        new_highlighter: &mut impl Highlighter,
    ) {
        self.with_internal_mut(|internal| {
            let mut font_system =
                text::font_system().write().expect("Write font system");

            let buffer = buffer_mut_from_editor(&mut internal.editor);

            if font_system.version() != internal.version {
                log::trace!("Updating `FontSystem` of `Editor`...");

                for line in buffer.lines.iter_mut() {
                    line.reset();
                }

                internal.version = font_system.version();
                internal.topmost_line_changed = Some(0);
            }

            if new_font != internal.font {
                log::trace!("Updating font of `Editor`...");

                for line in buffer.lines.iter_mut() {
                    let _ = line.set_attrs_list(cosmic_text::AttrsList::new(
                        &text::to_attributes(new_font),
                    ));
                }

                internal.font = new_font;
                internal.topmost_line_changed = Some(0);
            }

            let metrics = buffer.metrics();
            let new_line_height = new_line_height.to_absolute(new_size);

            if new_size.0 != metrics.font_size
                || new_line_height.0 != metrics.line_height
            {
                log::trace!("Updating `Metrics` of `Editor`...");

                buffer.set_metrics(
                    font_system.raw(),
                    cosmic_text::Metrics::new(new_size.0, new_line_height.0),
                );
            }

            let new_wrap = text::to_wrap(new_wrapping);

            if new_wrap != buffer.wrap() {
                log::trace!("Updating `Wrap` strategy of `Editor`...");

                buffer.set_wrap(font_system.raw(), new_wrap);
            }

            if new_bounds != internal.bounds {
                log::trace!("Updating size of `Editor`...");

                buffer.set_size(
                    font_system.raw(),
                    Some(new_bounds.width),
                    Some(new_bounds.height),
                );

                internal.bounds = new_bounds;
            }

            if let Some(topmost_line_changed) =
                internal.topmost_line_changed.take()
            {
                log::trace!(
                    "Notifying highlighter of line \
                    change: {topmost_line_changed}"
                );

                new_highlighter.change_line(topmost_line_changed);
            }

            internal.editor.shape_as_needed(font_system.raw(), false);

            // For `min_bounds`; lines already laid out are kept.
            let buffer = buffer_mut_from_editor(&mut internal.editor);

            for line in 0..buffer.lines.len() {
                let _ = buffer.line_layout(font_system.raw(), line);
            }
        });
    }

    fn highlight<H: Highlighter>(
        &mut self,
        font: Self::Font,
        highlighter: &mut H,
        format_highlight: impl Fn(&H::Highlight) -> highlighter::Format<Self::Font>,
    ) {
        let internal = self.internal();
        let buffer = buffer_from_editor(&internal.editor);

        let scroll = buffer.scroll();
        let mut window = (internal.bounds.height / buffer.metrics().line_height)
            .ceil() as i32;

        let last_visible_line = buffer.lines[scroll.line..]
            .iter()
            .enumerate()
            .find_map(|(i, line)| {
                let visible_lines = line
                    .layout_opt()
                    .as_ref()
                    .expect("Line layout should be cached")
                    .len() as i32;

                if window > visible_lines {
                    window -= visible_lines;
                    None
                } else {
                    Some(scroll.line + i)
                }
            })
            .unwrap_or(buffer.lines.len().saturating_sub(1));

        let current_line = highlighter.current_line();

        if current_line > last_visible_line {
            return;
        }

        let editor =
            self.0.take().expect("Editor should always be initialized");

        let mut internal = Arc::try_unwrap(editor)
            .expect("Editor cannot have multiple strong references");

        let mut font_system =
            text::font_system().write().expect("Write font system");

        let attributes = text::to_attributes(font);

        for line in &mut buffer_mut_from_editor(&mut internal.editor).lines
            [current_line..=last_visible_line]
        {
            let mut list = cosmic_text::AttrsList::new(&attributes);

            for (range, highlight) in highlighter.highlight_line(line.text()) {
                let format = format_highlight(&highlight);

                if format.color.is_some()
                    || format.font.is_some()
                    || format.underline.is_some()
                {
                    list.add_span(
                        range,
                        &cosmic_text::Attrs {
                            color_opt: format.color.map(text::to_color),
                            // Mtrx patch (spelling).
                            metadata: format.underline.map_or(0, |color| {
                                underline_metadata(
                                    &mut internal.underline_colors,
                                    color,
                                )
                            }),
                            ..if let Some(font) = format.font {
                                text::to_attributes(font)
                            } else {
                                attributes.clone()
                            }
                        },
                    );
                }
            }

            let _ = line.set_attrs_list(list);
        }

        internal.editor.shape_as_needed(font_system.raw(), false);

        self.0 = Some(Arc::new(internal));
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self(Some(Arc::new(Internal::default())))
    }
}

impl PartialEq for Internal {
    fn eq(&self, other: &Self) -> bool {
        self.font == other.font
            && self.bounds == other.bounds
            && buffer_from_editor(&self.editor).metrics()
                == buffer_from_editor(&other.editor).metrics()
    }
}

impl Default for Internal {
    fn default() -> Self {
        Self {
            editor: cosmic_text::Editor::new(cosmic_text::Buffer::new_empty(
                cosmic_text::Metrics {
                    font_size: 1.0,
                    line_height: 1.0,
                },
            )),
            selection: RwLock::new(None),
            font: Font::default(),
            bounds: Size::ZERO,
            topmost_line_changed: None,
            version: text::Version::default(),
            // Mtrx patch (editing).
            history: history::History::default(),
            // Mtrx patch (spelling).
            underline_colors: Vec::new(),
        }
    }
}

impl fmt::Debug for Internal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Internal")
            .field("font", &self.font)
            .field("bounds", &self.bounds)
            .finish()
    }
}

/// A weak reference to an [`Editor`].
#[derive(Debug, Clone)]
pub struct Weak {
    raw: sync::Weak<Internal>,
    /// The bounds of the [`Editor`].
    pub bounds: Size,
}

impl Weak {
    /// Tries to update the reference into an [`Editor`].
    pub fn upgrade(&self) -> Option<Editor> {
        self.raw.upgrade().map(Some).map(Editor)
    }
}

impl PartialEq for Weak {
    fn eq(&self, other: &Self) -> bool {
        match (self.raw.upgrade(), other.raw.upgrade()) {
            (Some(p1), Some(p2)) => p1 == p2,
            _ => false,
        }
    }
}

fn highlight_line(
    line: &cosmic_text::BufferLine,
    from: usize,
    to: usize,
) -> impl Iterator<Item = (f32, f32)> + '_ {
    let layout = line.layout_opt().map(Vec::as_slice).unwrap_or_default();

    layout.iter().map(move |visual_line| {
        let start = visual_line
            .glyphs
            .first()
            .map(|glyph| glyph.start)
            .unwrap_or(0);
        let end = visual_line
            .glyphs
            .last()
            .map(|glyph| glyph.end)
            .unwrap_or(0);

        let range = start.max(from)..end.min(to);

        if range.is_empty() {
            (0.0, 0.0)
        } else if range.start == start && range.end == end {
            (0.0, visual_line.w)
        } else {
            let first_glyph = visual_line
                .glyphs
                .iter()
                .position(|glyph| range.start <= glyph.start)
                .unwrap_or(0);

            let mut glyphs = visual_line.glyphs.iter();

            let x =
                glyphs.by_ref().take(first_glyph).map(|glyph| glyph.w).sum();

            let width: f32 = glyphs
                .take_while(|glyph| range.end > glyph.start)
                .map(|glyph| glyph.w)
                .sum();

            (x, width)
        }
    })
}

// Mtrx patch (editing): an edit, out of `perform` so that the history can
// record it.
fn perform_edit(
    editor: &mut cosmic_text::Editor<'static>,
    font_system: &mut text::FontSystem,
    edit: Edit,
) {
    match edit {
        Edit::Insert(c) => {
            editor.action(font_system.raw(), cosmic_text::Action::Insert(c));
        }
        Edit::Paste(text) => {
            editor.insert_string(&text, None);
        }
        Edit::Indent => {
            editor.action(font_system.raw(), cosmic_text::Action::Indent);
        }
        Edit::Unindent => {
            editor.action(font_system.raw(), cosmic_text::Action::Unindent);
        }
        Edit::Enter => {
            editor.action(font_system.raw(), cosmic_text::Action::Enter);
        }
        Edit::Backspace => {
            editor.action(font_system.raw(), cosmic_text::Action::Backspace);
        }
        Edit::Delete => {
            editor.action(font_system.raw(), cosmic_text::Action::Delete);
        }
        // Mtrx patch (surrounding text).
        Edit::DeleteSurrounding { before, after } => {
            delete_surrounding(editor, before, after);
        }
        // Replayed by the history.
        Edit::Undo | Edit::Redo => {}
        Edit::DeleteTo(motion) => {
            delete_to(editor, font_system, motion);
        }
        Edit::Kill => kill(editor, font_system),
        Edit::Yank => yank(editor),
        Edit::Transpose => transpose(editor),
        Edit::OpenLine => open_line(editor),
        Edit::MoveSelection { to, copy } => move_selection(editor, to, copy),
    }
}

// Mtrx patch (editing): ⌃K's text, for ⌃Y; one for the app, as macOS has.
static KILLED: sync::Mutex<String> = sync::Mutex::new(String::new());

/// Deletes the selection, or to the end of the paragraph (its line ending
/// when there), and keeps it for [`yank`].
fn kill(
    editor: &mut cosmic_text::Editor<'static>,
    font_system: &mut text::FontSystem,
) {
    if editor.selection_bounds().is_none() {
        let cursor = editor.cursor();

        perform_motion(editor, font_system, Motion::ParagraphEnd);

        if editor.cursor() == cursor {
            editor.action(
                font_system.raw(),
                cosmic_text::Action::Motion(cosmic_text::Motion::Next),
            );
        }

        editor.set_selection(cosmic_text::Selection::Normal(cursor));
    }

    let killed = editor.copy_selection().unwrap_or_default();

    if killed.is_empty() {
        editor.set_selection(cosmic_text::Selection::None);
        return;
    }

    let _ = editor.delete_selection();
    *KILLED.lock().expect("Lock kill buffer") = killed;
}

/// Inserts the text last killed, in place of the selection.
fn yank(editor: &mut cosmic_text::Editor<'static>) {
    let killed = KILLED.lock().expect("Lock kill buffer").clone();

    if !killed.is_empty() {
        editor.insert_string(&killed, None);
    }
}

/// Swaps the characters (graphemes) around the cursor, or the two before
/// it at the end of a line, and moves after them.
fn transpose(editor: &mut cosmic_text::Editor<'static>) {
    use unicode_segmentation::UnicodeSegmentation;

    if editor.selection_bounds().is_some() {
        return;
    }

    let cursor = editor.cursor();
    let text = buffer_from_editor(editor).lines[cursor.line].text();
    let graphemes: Vec<_> = text.grapheme_indices(true).collect();

    // The grapheme after the cursor: at the end of the line, the last.
    let Some(after) = graphemes
        .iter()
        .position(|(i, _)| *i >= cursor.index)
        .or_else(|| graphemes.len().checked_sub(1))
        .filter(|after| *after > 0)
    else {
        return;
    };

    let (start, before) = graphemes[after - 1];
    let (at, after) = graphemes[after];
    let swapped = format!("{after}{before}");
    let end = at + after.len();

    let at = |index| cosmic_text::Cursor { index, ..cursor };

    editor.delete_range(at(start), at(end));
    let moved = editor.insert_at(at(start), &swapped, None);
    editor.set_cursor(moved);
}

/// Moves the selection to where a click at `to` would put the caret, or
/// copies it there, unless that is in it; it stays selected.
fn move_selection(
    editor: &mut cosmic_text::Editor<'static>,
    to: Point,
    copy: bool,
) {
    let Some((start, end)) = editor.selection_bounds() else {
        return;
    };
    let Some(mut target) = buffer_from_editor(editor).hit(to.x, to.y) else {
        return;
    };

    let at = |cursor: cosmic_text::Cursor| (cursor.line, cursor.index);

    if at(start) <= at(target) && at(target) <= at(end) {
        return;
    }

    let Some(text) = editor.copy_selection() else {
        return;
    };

    editor.set_selection(cosmic_text::Selection::None);

    if !copy {
        editor.delete_range(start, end);

        // After the selection, it moves back by it.
        if at(target) > at(end) {
            target = if target.line == end.line {
                cosmic_text::Cursor {
                    line: start.line,
                    index: start.index + (target.index - end.index),
                    ..target
                }
            } else {
                cosmic_text::Cursor {
                    line: target.line - (end.line - start.line),
                    ..target
                }
            };
        }
    }

    let moved = editor.insert_at(target, &text, None);
    editor.set_cursor(moved);
    editor.set_selection(cosmic_text::Selection::Normal(target));
}

/// Breaks the line after the cursor, in place of the selection; the
/// cursor stays.
fn open_line(editor: &mut cosmic_text::Editor<'static>) {
    let _ = editor.delete_selection();

    let cursor = editor.cursor();
    let _ = editor.insert_at(cursor, "\n", None);
    editor.set_cursor(cursor);
}

// Mtrx patch (editing): ⌥⌫, ⌘⌫ and the like.
/// Deletes the selection, or from the cursor to where `motion` takes it; a
/// character if it takes it nowhere (a line's edge, where ⌘⌫ joins the
/// line to the one before, as a text view does).
fn delete_to(
    editor: &mut cosmic_text::Editor<'static>,
    font_system: &mut text::FontSystem,
    motion: Motion,
) {
    if editor.delete_selection() {
        return;
    }

    let cursor = editor.cursor();
    perform_motion(editor, font_system, motion);
    let to = editor.cursor();
    editor.set_cursor(cursor);

    let at = |cursor: cosmic_text::Cursor| (cursor.line, cursor.index);

    if at(to) == at(cursor) {
        editor.action(
            font_system.raw(),
            match motion.direction() {
                Direction::Left => cosmic_text::Action::Backspace,
                Direction::Right => cosmic_text::Action::Delete,
            },
        );
        return;
    }

    let (start, end) = if at(to) < at(cursor) {
        (to, cursor)
    } else {
        (cursor, to)
    };

    editor.set_cursor(start);
    editor.delete_range(start, end);
}

// Mtrx patch (spelling).
/// The glyph metadata of an underline in `color`: one more than its index
/// in `colors`, where it is added if new.
fn underline_metadata(colors: &mut Vec<Color>, color: Color) -> usize {
    let index = colors.iter().position(|c| *c == color).unwrap_or_else(|| {
        colors.push(color);
        colors.len() - 1
    });

    index + 1
}

/// The color of the underline of a glyph with `metadata`, if any.
fn underline_color(colors: &[Color], metadata: usize) -> Option<Color> {
    colors.get(metadata.checked_sub(1)?).copied()
}

// Mtrx patch (editing): out of `selection`, for `caret_at` too.
/// Where the caret is drawn for `cursor`.
fn caret(buffer: &cosmic_text::Buffer, cursor: cosmic_text::Cursor) -> Point {
    let line_height = buffer.metrics().line_height;

    let visual_lines_offset = visual_lines_offset(cursor.line, buffer);

    let line = buffer
        .lines
        .get(cursor.line)
        .expect("Cursor line should be present");

    let layout = line.layout_opt().expect("Line layout should be cached");

    let mut lines = layout.iter().enumerate();

    let (visual_line, offset) = lines
        .find_map(|(i, line)| {
            let start =
                line.glyphs.first().map(|glyph| glyph.start).unwrap_or(0);
            let end = line.glyphs.last().map(|glyph| glyph.end).unwrap_or(0);

            let is_cursor_before_start = start > cursor.index;

            let is_cursor_before_end = match cursor.affinity {
                cosmic_text::Affinity::Before => cursor.index <= end,
                cosmic_text::Affinity::After => cursor.index < end,
            };

            if is_cursor_before_start {
                // Sometimes, the glyph we are looking for is right
                // between lines. This can happen when a line wraps
                // on a space.
                // In that case, we can assume the cursor is at the
                // end of the previous line.
                // i is guaranteed to be > 0 because `start` is always
                // 0 for the first line, so there is no way for the
                // cursor to be before it.
                Some((i - 1, layout[i - 1].w))
            } else if is_cursor_before_end {
                let offset = line
                    .glyphs
                    .iter()
                    .take_while(|glyph| cursor.index > glyph.start)
                    .map(|glyph| glyph.w)
                    .sum();

                Some((i, offset))
            } else {
                None
            }
        })
        .unwrap_or((
            layout.len().saturating_sub(1),
            layout.last().map(|line| line.w).unwrap_or(0.0),
        ));

    Point::new(
        offset,
        (visual_lines_offset + visual_line as i32) as f32 * line_height
            - buffer.scroll().vertical,
    )
}

fn visual_lines_offset(line: usize, buffer: &cosmic_text::Buffer) -> i32 {
    let scroll = buffer.scroll();

    let start = scroll.line.min(line);
    let end = scroll.line.max(line);

    let visual_lines_offset: usize = buffer.lines[start..]
        .iter()
        .take(end - start)
        .map(|line| line.layout_opt().map(Vec::len).unwrap_or_default())
        .sum();

    visual_lines_offset as i32 * if scroll.line < line { 1 } else { -1 }
}

// Mtrx patch (editing): a motion may take more than one of cosmic-text's.
fn perform_motion(
    editor: &mut cosmic_text::Editor<'static>,
    font_system: &mut text::FontSystem,
    motion: Motion,
) {
    for motion in to_motions(motion) {
        editor.action(font_system.raw(), cosmic_text::Action::Motion(*motion));
    }
}

fn to_motions(motion: Motion) -> &'static [cosmic_text::Motion] {
    match motion {
        Motion::Left => &[cosmic_text::Motion::Left],
        Motion::Right => &[cosmic_text::Motion::Right],
        Motion::Up => &[cosmic_text::Motion::Up],
        Motion::Down => &[cosmic_text::Motion::Down],
        Motion::WordLeft => &[cosmic_text::Motion::LeftWord],
        Motion::WordRight => &[cosmic_text::Motion::RightWord],
        Motion::Home => &[cosmic_text::Motion::Home],
        Motion::End => &[cosmic_text::Motion::End],
        Motion::PageUp => &[cosmic_text::Motion::PageUp],
        Motion::PageDown => &[cosmic_text::Motion::PageDown],
        Motion::DocumentStart => &[cosmic_text::Motion::BufferStart],
        Motion::DocumentEnd => &[cosmic_text::Motion::BufferEnd],
        // Back a character first, so that from a paragraph's start it goes
        // to the start of the one before, as a text view does.
        Motion::ParagraphBackward => &[
            cosmic_text::Motion::Previous,
            cosmic_text::Motion::ParagraphStart,
        ],
        Motion::ParagraphForward => {
            &[cosmic_text::Motion::Next, cosmic_text::Motion::ParagraphEnd]
        }
        Motion::ParagraphStart => &[cosmic_text::Motion::ParagraphStart],
        Motion::ParagraphEnd => &[cosmic_text::Motion::ParagraphEnd],
    }
}

// Mtrx patch (surrounding text): an input method's `DeleteSurrounding`.
/// Deletes `before` and `after` bytes around the selection (or the cursor),
/// a line ending counting as its bytes, and keeps the selection. A count
/// ending inside a character takes all of it.
fn delete_surrounding(
    editor: &mut cosmic_text::Editor<'static>,
    before: usize,
    after: usize,
) {
    let cursor = editor.cursor();
    let (start, end) = editor.selection_bounds().unwrap_or((cursor, cursor));

    let buffer = buffer_from_editor(editor);
    let from = back_by(buffer, start, before);
    let to = forward_by(buffer, end, after);

    // After the selection first, so that what precedes it stays put.
    if to != end {
        editor.delete_range(end, to);
    }

    if from == start {
        return;
    }

    editor.delete_range(from, start);

    // The cursor and the selection's anchor, at or after `start`, move
    // back by what was deleted before them.
    let shift = |at: cosmic_text::Cursor| {
        if at.line == start.line {
            cosmic_text::Cursor {
                line: from.line,
                index: from.index + at.index.saturating_sub(start.index),
                ..at
            }
        } else {
            cosmic_text::Cursor {
                line: at.line - (start.line - from.line),
                ..at
            }
        }
    };

    let selection = match editor.selection() {
        cosmic_text::Selection::None => cosmic_text::Selection::None,
        cosmic_text::Selection::Normal(anchor) => {
            cosmic_text::Selection::Normal(shift(anchor))
        }
        cosmic_text::Selection::Line(anchor) => {
            cosmic_text::Selection::Line(shift(anchor))
        }
        cosmic_text::Selection::Word(anchor) => {
            cosmic_text::Selection::Word(shift(anchor))
        }
    };

    editor.set_cursor(shift(cursor));
    editor.set_selection(selection);
}

/// The position `bytes` before `at`, across line endings.
fn back_by(
    buffer: &cosmic_text::Buffer,
    mut at: cosmic_text::Cursor,
    mut bytes: usize,
) -> cosmic_text::Cursor {
    loop {
        if bytes <= at.index {
            let text = buffer.lines[at.line].text();
            at.index -= bytes;

            while !text.is_char_boundary(at.index) {
                at.index -= 1;
            }

            return at;
        }

        bytes -= at.index;
        at.index = 0;

        if at.line == 0 {
            return at;
        }

        let previous = &buffer.lines[at.line - 1];
        bytes = bytes.saturating_sub(previous.ending().as_str().len());
        at = cosmic_text::Cursor {
            line: at.line - 1,
            index: previous.text().len(),
            ..at
        };
    }
}

/// The position `bytes` after `at`, across line endings.
fn forward_by(
    buffer: &cosmic_text::Buffer,
    mut at: cosmic_text::Cursor,
    mut bytes: usize,
) -> cosmic_text::Cursor {
    loop {
        let line = &buffer.lines[at.line];
        let text = line.text();
        let rest = text.len() - at.index;

        if bytes <= rest {
            at.index += bytes;

            while !text.is_char_boundary(at.index) {
                at.index += 1;
            }

            return at;
        }

        bytes -= rest;
        at.index = text.len();

        if at.line + 1 == buffer.lines.len() {
            return at;
        }

        bytes = bytes.saturating_sub(line.ending().as_str().len());
        at = cosmic_text::Cursor {
            line: at.line + 1,
            index: 0,
            ..at
        };
    }
}

fn buffer_from_editor<'a, 'b>(
    editor: &'a impl cosmic_text::Edit<'b>,
) -> &'a cosmic_text::Buffer
where
    'b: 'a,
{
    match editor.buffer_ref() {
        cosmic_text::BufferRef::Owned(buffer) => buffer,
        cosmic_text::BufferRef::Borrowed(buffer) => buffer,
        cosmic_text::BufferRef::Arc(buffer) => buffer,
    }
}

fn buffer_mut_from_editor<'a, 'b>(
    editor: &'a mut impl cosmic_text::Edit<'b>,
) -> &'a mut cosmic_text::Buffer
where
    'b: 'a,
{
    match editor.buffer_ref_mut() {
        cosmic_text::BufferRef::Owned(buffer) => buffer,
        cosmic_text::BufferRef::Borrowed(buffer) => buffer,
        cosmic_text::BufferRef::Arc(_buffer) => unreachable!(),
    }
}
