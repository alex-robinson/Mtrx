//! Text editors display a multi-line text input for text editing.
//!
//! # Example
//! ```no_run
//! # mod iced { pub mod widget { pub use iced_widget::*; } pub use iced_widget::Renderer; pub use iced_widget::core::*; }
//! # pub type Element<'a, Message> = iced_widget::core::Element<'a, Message, iced_widget::Theme, iced_widget::Renderer>;
//! #
//! use iced::widget::text_editor;
//!
//! struct State {
//!    content: text_editor::Content,
//! }
//!
//! #[derive(Debug, Clone)]
//! enum Message {
//!     Edit(text_editor::Action)
//! }
//!
//! fn view(state: &State) -> Element<'_, Message> {
//!     text_editor(&state.content)
//!         .placeholder("Type something here...")
//!         .on_action(Message::Edit)
//!         .into()
//! }
//!
//! fn update(state: &mut State, message: Message) {
//!     match message {
//!         Message::Edit(action) => {
//!             state.content.perform(action);
//!         }
//!     }
//! }
//! ```
use crate::core::alignment;
use crate::core::clipboard::{self, Clipboard};
use crate::core::input_method;
use crate::core::keyboard;
use crate::core::keyboard::key;
use crate::core::layout::{self, Layout};
use crate::core::mouse;
use crate::core::renderer;
use crate::core::text::editor::Editor as _;
use crate::core::text::highlighter::{self, Highlighter};
use crate::core::text::{self, LineHeight, Text, Wrapping};
use crate::core::theme;
use crate::core::time::{Duration, Instant};
use crate::core::widget::operation;
use crate::core::widget::{self, Widget};
use crate::core::window;
use crate::core::{
    Background, Border, Color, Element, Event, InputMethod, Length, Padding,
    Pixels, Point, Rectangle, Shell, Size, SmolStr, Theme, Vector,
};

use std::borrow::Cow;
use std::cell::RefCell;
use std::fmt;
use std::ops;
use std::ops::DerefMut;
use std::sync::Arc;

pub use text::editor::{
    Action, Cursor, Edit, Line, LineEnding, Motion, Position, Selection,
};

/// A multi-line text input.
///
/// # Example
/// ```no_run
/// # mod iced { pub mod widget { pub use iced_widget::*; } pub use iced_widget::Renderer; pub use iced_widget::core::*; }
/// # pub type Element<'a, Message> = iced_widget::core::Element<'a, Message, iced_widget::Theme, iced_widget::Renderer>;
/// #
/// use iced::widget::text_editor;
///
/// struct State {
///    content: text_editor::Content,
/// }
///
/// #[derive(Debug, Clone)]
/// enum Message {
///     Edit(text_editor::Action)
/// }
///
/// fn view(state: &State) -> Element<'_, Message> {
///     text_editor(&state.content)
///         .placeholder("Type something here...")
///         .on_action(Message::Edit)
///         .into()
/// }
///
/// fn update(state: &mut State, message: Message) {
///     match message {
///         Message::Edit(action) => {
///             state.content.perform(action);
///         }
///     }
/// }
/// ```
pub struct TextEditor<
    'a,
    Highlighter,
    Message,
    Theme = crate::Theme,
    Renderer = crate::Renderer,
> where
    Highlighter: text::Highlighter,
    Theme: Catalog,
    Renderer: text::Renderer,
{
    id: Option<widget::Id>,
    content: &'a Content<Renderer>,
    placeholder: Option<text::Fragment<'a>>,
    font: Option<Renderer::Font>,
    text_size: Option<Pixels>,
    line_height: LineHeight,
    width: Length,
    height: Length,
    min_height: f32,
    max_height: f32,
    padding: Padding,
    wrapping: Wrapping,
    class: Theme::Class<'a>,
    key_binding: Option<Box<dyn Fn(KeyPress) -> Option<Binding<Message>> + 'a>>,
    on_edit: Option<Box<dyn Fn(Action) -> Message + 'a>>,
    highlighter_settings: Highlighter::Settings,
    highlighter_format: fn(
        &Highlighter::Highlight,
        &Theme,
    ) -> highlighter::Format<Renderer::Font>,
    last_status: Option<Status>,
}

impl<'a, Message, Theme, Renderer>
    TextEditor<'a, highlighter::PlainText, Message, Theme, Renderer>
where
    Theme: Catalog,
    Renderer: text::Renderer,
{
    /// Creates new [`TextEditor`] with the given [`Content`].
    pub fn new(content: &'a Content<Renderer>) -> Self {
        Self {
            id: None,
            content,
            placeholder: None,
            font: None,
            text_size: None,
            line_height: LineHeight::default(),
            width: Length::Fill,
            height: Length::Shrink,
            min_height: 0.0,
            max_height: f32::INFINITY,
            padding: Padding::new(5.0),
            wrapping: Wrapping::default(),
            class: <Theme as Catalog>::default(),
            key_binding: None,
            on_edit: None,
            highlighter_settings: (),
            highlighter_format: |_highlight, _theme| {
                highlighter::Format::default()
            },
            last_status: None,
        }
    }

    /// Sets the [`Id`](widget::Id) of the [`TextEditor`].
    pub fn id(mut self, id: impl Into<widget::Id>) -> Self {
        self.id = Some(id.into());
        self
    }
}

impl<'a, Highlighter, Message, Theme, Renderer>
    TextEditor<'a, Highlighter, Message, Theme, Renderer>
where
    Highlighter: text::Highlighter,
    Theme: Catalog,
    Renderer: text::Renderer,
{
    /// Sets the placeholder of the [`TextEditor`].
    pub fn placeholder(
        mut self,
        placeholder: impl text::IntoFragment<'a>,
    ) -> Self {
        self.placeholder = Some(placeholder.into_fragment());
        self
    }

    /// Sets the width of the [`TextEditor`].
    pub fn width(mut self, width: impl Into<Pixels>) -> Self {
        self.width = Length::from(width.into());
        self
    }

    /// Sets the height of the [`TextEditor`].
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    /// Sets the minimum height of the [`TextEditor`].
    pub fn min_height(mut self, min_height: impl Into<Pixels>) -> Self {
        self.min_height = min_height.into().0;
        self
    }

    /// Sets the maximum height of the [`TextEditor`].
    pub fn max_height(mut self, max_height: impl Into<Pixels>) -> Self {
        self.max_height = max_height.into().0;
        self
    }

    /// Sets the message that should be produced when some action is performed in
    /// the [`TextEditor`].
    ///
    /// If this method is not called, the [`TextEditor`] will be disabled.
    pub fn on_action(
        mut self,
        on_edit: impl Fn(Action) -> Message + 'a,
    ) -> Self {
        self.on_edit = Some(Box::new(on_edit));
        self
    }

    /// Sets the [`Font`] of the [`TextEditor`].
    ///
    /// [`Font`]: text::Renderer::Font
    pub fn font(mut self, font: impl Into<Renderer::Font>) -> Self {
        self.font = Some(font.into());
        self
    }

    /// Sets the text size of the [`TextEditor`].
    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.text_size = Some(size.into());
        self
    }

    /// Sets the [`text::LineHeight`] of the [`TextEditor`].
    pub fn line_height(
        mut self,
        line_height: impl Into<text::LineHeight>,
    ) -> Self {
        self.line_height = line_height.into();
        self
    }

    /// Sets the [`Padding`] of the [`TextEditor`].
    pub fn padding(mut self, padding: impl Into<Padding>) -> Self {
        self.padding = padding.into();
        self
    }

    /// Sets the [`Wrapping`] strategy of the [`TextEditor`].
    pub fn wrapping(mut self, wrapping: Wrapping) -> Self {
        self.wrapping = wrapping;
        self
    }

    /// Highlights the [`TextEditor`] using the given syntax and theme.
    #[cfg(feature = "highlighter")]
    pub fn highlight(
        self,
        syntax: &str,
        theme: iced_highlighter::Theme,
    ) -> TextEditor<'a, iced_highlighter::Highlighter, Message, Theme, Renderer>
    where
        Renderer: text::Renderer<Font = crate::core::Font>,
    {
        self.highlight_with::<iced_highlighter::Highlighter>(
            iced_highlighter::Settings {
                theme,
                token: syntax.to_owned(),
            },
            |highlight, _theme| highlight.to_format(),
        )
    }

    /// Highlights the [`TextEditor`] with the given [`Highlighter`] and
    /// a strategy to turn its highlights into some text format.
    pub fn highlight_with<H: text::Highlighter>(
        self,
        settings: H::Settings,
        to_format: fn(
            &H::Highlight,
            &Theme,
        ) -> highlighter::Format<Renderer::Font>,
    ) -> TextEditor<'a, H, Message, Theme, Renderer> {
        TextEditor {
            id: self.id,
            content: self.content,
            placeholder: self.placeholder,
            font: self.font,
            text_size: self.text_size,
            line_height: self.line_height,
            width: self.width,
            height: self.height,
            min_height: self.min_height,
            max_height: self.max_height,
            padding: self.padding,
            wrapping: self.wrapping,
            class: self.class,
            key_binding: self.key_binding,
            on_edit: self.on_edit,
            highlighter_settings: settings,
            highlighter_format: to_format,
            last_status: self.last_status,
        }
    }

    /// Sets the closure to produce key bindings on key presses.
    ///
    /// See [`Binding`] for the list of available bindings.
    pub fn key_binding(
        mut self,
        key_binding: impl Fn(KeyPress) -> Option<Binding<Message>> + 'a,
    ) -> Self {
        self.key_binding = Some(Box::new(key_binding));
        self
    }

    /// Sets the style of the [`TextEditor`].
    #[must_use]
    pub fn style(mut self, style: impl Fn(&Theme, Status) -> Style + 'a) -> Self
    where
        Theme::Class<'a>: From<StyleFn<'a, Theme>>,
    {
        self.class = (Box::new(style) as StyleFn<'a, Theme>).into();
        self
    }

    /// Sets the style class of the [`TextEditor`].
    #[cfg(feature = "advanced")]
    #[must_use]
    pub fn class(mut self, class: impl Into<Theme::Class<'a>>) -> Self {
        self.class = class.into();
        self
    }

    fn input_method<'b>(
        &self,
        state: &'b State<Highlighter>,
        renderer: &Renderer,
        layout: Layout<'_>,
    ) -> InputMethod<&'b str> {
        // Mtrx patch: by the editor's focus, not the window's. The system's
        // Emoji & Symbols takes the key window while it is open and types the
        // pick into the field focused before; the input method off then
        // dropped it. A text view stays the first responder in a window that
        // is not key, too.
        if state.focus.is_none() {
            return InputMethod::Disabled;
        }

        // Mtrx patch (surrounding text).
        let surrounding_text = self.content.surrounding_text();

        let bounds = layout.bounds();
        let internal = self.content.0.borrow_mut();

        let text_bounds = bounds.shrink(self.padding);
        let translation = text_bounds.position() - Point::ORIGIN;

        let cursor = match internal.editor.selection() {
            Selection::Caret(position) => position,
            Selection::Range(ranges) => {
                ranges.first().cloned().unwrap_or_default().position()
            }
        };

        let line_height = self.line_height.to_absolute(
            self.text_size.unwrap_or_else(|| renderer.default_size()),
        );

        let position = cursor + translation;

        // Mtrx patch (menu actions).
        let edit = input_method::EditState {
            can_undo: internal.editor.can_undo(),
            can_redo: internal.editor.can_redo(),
            has_selection: matches!(
                internal.editor.selection(),
                Selection::Range(_)
            ),
        };

        InputMethod::Enabled {
            cursor: Rectangle::new(
                position,
                Size::new(1.0, f32::from(line_height)),
            ),
            purpose: input_method::Purpose::Normal,
            preedit: state.preedit.as_ref().map(input_method::Preedit::as_ref),
            // Mtrx patch (surrounding text).
            surrounding_text,
            // Mtrx patch (menu actions).
            edit,
        }
    }
}

/// The content of a [`TextEditor`].
pub struct Content<R = crate::Renderer>(RefCell<Internal<R>>)
where
    R: text::Renderer;

struct Internal<R>
where
    R: text::Renderer,
{
    editor: R::Editor,
}

impl<R> Content<R>
where
    R: text::Renderer,
{
    /// Creates an empty [`Content`].
    pub fn new() -> Self {
        Self::with_text("")
    }

    /// Creates a [`Content`] with the given text.
    pub fn with_text(text: &str) -> Self {
        Self(RefCell::new(Internal {
            editor: R::Editor::with_text(text),
        }))
    }

    /// Performs an [`Action`] on the [`Content`].
    pub fn perform(&mut self, action: Action) {
        let internal = self.0.get_mut();

        internal.editor.perform(action);
    }

    /// Moves the current cursor to reflect the given one.
    pub fn move_to(&mut self, cursor: Cursor) {
        let internal = self.0.get_mut();

        internal.editor.move_to(cursor);
    }

    /// Returns the current cursor position of the [`Content`].
    pub fn cursor(&self) -> Cursor {
        self.0.borrow().editor.cursor()
    }

    // Mtrx patch (surrounding text).
    /// Returns the text around the cursor for an input method: the lines
    /// with the cursor and the selection, cut to
    /// [`SurroundingText::MAX_BYTES`] around them.
    ///
    /// [`SurroundingText::MAX_BYTES`]: input_method::SurroundingText::MAX_BYTES
    pub fn surrounding_text(&self) -> Option<input_method::SurroundingText> {
        let internal = self.0.borrow();
        let Cursor {
            position,
            selection,
        } = internal.editor.cursor();
        let anchor = selection.unwrap_or(position);

        let mut text = String::new();
        let (mut cursor, mut anchor_at) = (0, 0);
        let last = position.line.max(anchor.line);

        for index in position.line.min(anchor.line)..=last {
            let line = internal.editor.line(index)?;

            if index == position.line {
                cursor = text.len() + position.column;
            }
            if index == anchor.line {
                anchor_at = text.len() + anchor.column;
            }

            text.push_str(&line.text);

            if index < last {
                text.push_str(line.ending.as_str());
            }
        }

        input_method::SurroundingText::around(&text, cursor, anchor_at)
    }

    // Mtrx patch (editing).
    /// Returns whether the selection, as drawn, contains the [`Point`]
    /// (relative to the text).
    fn selection_contains(&self, point: Point) -> bool {
        match self.0.borrow().editor.selection() {
            Selection::Range(ranges) => {
                ranges.iter().any(|range| range.contains(point))
            }
            Selection::Caret(_) => false,
        }
    }

    /// Returns the amount of lines of the [`Content`].
    pub fn line_count(&self) -> usize {
        self.0.borrow().editor.line_count()
    }

    /// Returns the text of the line at the given index, if it exists.
    pub fn line(&self, index: usize) -> Option<Line<'_>> {
        let internal = self.0.borrow();
        let line = internal.editor.line(index)?;

        Some(Line {
            text: Cow::Owned(line.text.into_owned()),
            ending: line.ending,
        })
    }

    /// Returns an iterator of the text of the lines in the [`Content`].
    pub fn lines(&self) -> impl Iterator<Item = Line<'_>> {
        (0..)
            .map(|i| self.line(i))
            .take_while(Option::is_some)
            .flatten()
    }

    /// Returns the text of the [`Content`].
    pub fn text(&self) -> String {
        let mut contents = String::new();
        let mut lines = self.lines().peekable();

        while let Some(line) = lines.next() {
            contents.push_str(&line.text);

            if lines.peek().is_some() {
                contents.push_str(if line.ending == LineEnding::None {
                    LineEnding::default().as_str()
                } else {
                    line.ending.as_str()
                });
            }
        }

        contents
    }

    /// Returns the selected text of the [`Content`].
    pub fn selection(&self) -> Option<String> {
        self.0.borrow().editor.copy()
    }

    /// Returns the kind of [`LineEnding`] used for separating lines in the [`Content`].
    pub fn line_ending(&self) -> Option<LineEnding> {
        Some(self.line(0)?.ending)
    }

    /// Returns whether or not the the [`Content`] is empty.
    pub fn is_empty(&self) -> bool {
        self.0.borrow().editor.is_empty()
    }
}

impl<Renderer> Clone for Content<Renderer>
where
    Renderer: text::Renderer,
{
    fn clone(&self) -> Self {
        Self::with_text(&self.text())
    }
}

impl<Renderer> Default for Content<Renderer>
where
    Renderer: text::Renderer,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<Renderer> fmt::Debug for Content<Renderer>
where
    Renderer: text::Renderer,
    Renderer::Editor: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let internal = self.0.borrow();

        f.debug_struct("Content")
            .field("editor", &internal.editor)
            .finish()
    }
}

/// The state of a [`TextEditor`].
#[derive(Debug)]
pub struct State<Highlighter: text::Highlighter> {
    focus: Option<Focus>,
    preedit: Option<input_method::Preedit>,
    last_click: Option<mouse::Click>,
    drag_click: Option<mouse::click::Kind>,
    partial_scroll: f32,
    last_theme: RefCell<Option<String>>,
    highlighter: RefCell<Highlighter>,
    highlighter_settings: Highlighter::Settings,
    highlighter_format_address: usize,
    // Mtrx patch (editing): the selection being dragged, and the modifiers
    // (⌥ copies it).
    drag_text: Option<DragText>,
    modifiers: keyboard::Modifiers,
}

/// Mtrx patch (editing): the selection pressed, to be dragged.
#[derive(Debug, Clone, Copy)]
struct DragText {
    /// Where it was pressed.
    from: Point,
    /// Where it would be dropped: the pointer, once it moved, while it is
    /// over the editor.
    to: Option<Point>,
    /// Whether the pointer moved from where it was pressed: else releasing
    /// it is a click.
    moved: bool,
}

impl DragText {
    /// How far the pointer moves before the selection is dragged.
    const THRESHOLD: f32 = 3.0;
}

/// Mtrx patch (editing): whether a drop with `modifiers` copies: ⌥ on
/// macOS, Ctrl elsewhere.
fn copies(modifiers: keyboard::Modifiers) -> bool {
    if cfg!(target_os = "macos") {
        modifiers.alt()
    } else {
        modifiers.control()
    }
}

#[derive(Debug, Clone)]
struct Focus {
    updated_at: Instant,
    now: Instant,
    is_window_focused: bool,
}

impl Focus {
    const CURSOR_BLINK_INTERVAL_MILLIS: u128 = 500;

    fn now() -> Self {
        let now = Instant::now();

        Self {
            updated_at: now,
            now,
            is_window_focused: true,
        }
    }

    fn is_cursor_visible(&self) -> bool {
        self.is_window_focused
            && ((self.now - self.updated_at).as_millis()
                / Self::CURSOR_BLINK_INTERVAL_MILLIS)
                .is_multiple_of(2)
    }
}

impl<Highlighter: text::Highlighter> State<Highlighter> {
    /// Returns whether the [`TextEditor`] is currently focused or not.
    pub fn is_focused(&self) -> bool {
        self.focus.is_some()
    }
}

impl<Highlighter: text::Highlighter> operation::Focusable
    for State<Highlighter>
{
    fn is_focused(&self) -> bool {
        self.focus.is_some()
    }

    fn focus(&mut self) {
        self.focus = Some(Focus::now());
    }

    fn unfocus(&mut self) {
        self.focus = None;
    }
}

impl<Highlighter, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for TextEditor<'_, Highlighter, Message, Theme, Renderer>
where
    Highlighter: text::Highlighter,
    Theme: Catalog,
    Renderer: text::Renderer,
{
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<State<Highlighter>>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State {
            focus: None,
            preedit: None,
            last_click: None,
            drag_click: None,
            partial_scroll: 0.0,
            // Mtrx patch (editing).
            drag_text: None,
            modifiers: keyboard::Modifiers::default(),
            last_theme: RefCell::default(),
            highlighter: RefCell::new(Highlighter::new(
                &self.highlighter_settings,
            )),
            highlighter_settings: self.highlighter_settings.clone(),
            highlighter_format_address: self.highlighter_format as usize,
        })
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> iced_renderer::core::layout::Node {
        let mut internal = self.content.0.borrow_mut();
        let state = tree.state.downcast_mut::<State<Highlighter>>();

        if state.highlighter_format_address != self.highlighter_format as usize
        {
            state.highlighter.borrow_mut().change_line(0);

            state.highlighter_format_address = self.highlighter_format as usize;
        }

        if state.highlighter_settings != self.highlighter_settings {
            state
                .highlighter
                .borrow_mut()
                .update(&self.highlighter_settings);

            state.highlighter_settings = self.highlighter_settings.clone();
        }

        let limits = limits
            .width(self.width)
            .height(self.height)
            .min_height(self.min_height)
            .max_height(self.max_height);

        internal.editor.update(
            limits.shrink(self.padding).max(),
            self.font.unwrap_or_else(|| renderer.default_font()),
            self.text_size.unwrap_or_else(|| renderer.default_size()),
            self.line_height,
            self.wrapping,
            state.highlighter.borrow_mut().deref_mut(),
        );

        match self.height {
            Length::Fill | Length::FillPortion(_) | Length::Fixed(_) => {
                layout::Node::new(limits.max())
            }
            Length::Shrink => {
                let min_bounds = internal.editor.min_bounds();

                layout::Node::new(
                    limits
                        .height(min_bounds.height)
                        .max()
                        .expand(Size::new(0.0, self.padding.y())),
                )
            }
        }
    }

    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let Some(on_edit) = self.on_edit.as_ref() else {
            return;
        };

        let state = tree.state.downcast_mut::<State<Highlighter>>();
        let is_redraw = matches!(
            event,
            Event::Window(window::Event::RedrawRequested(_now)),
        );

        match event {
            // Mtrx patch (editing).
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.modifiers = *modifiers;
            }
            Event::Window(window::Event::Unfocused) => {
                if let Some(focus) = &mut state.focus {
                    focus.is_window_focused = false;
                }
            }
            Event::Window(window::Event::Focused) => {
                if let Some(focus) = &mut state.focus {
                    focus.is_window_focused = true;
                    focus.updated_at = Instant::now();

                    shell.request_redraw();
                }
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                if let Some(focus) = &mut state.focus
                    && focus.is_window_focused
                {
                    focus.now = *now;

                    let millis_until_redraw =
                        Focus::CURSOR_BLINK_INTERVAL_MILLIS
                            - (focus.now - focus.updated_at).as_millis()
                                % Focus::CURSOR_BLINK_INTERVAL_MILLIS;

                    shell.request_redraw_at(
                        focus.now
                            + Duration::from_millis(millis_until_redraw as u64),
                    );
                }
            }
            _ => {}
        }

        if let Some(update) = Update::from_event(
            event,
            state,
            layout.bounds(),
            self.padding,
            cursor,
            self.key_binding.as_deref(),
        ) {
            match update {
                Update::Click(click) => {
                    let action = match click.kind() {
                        mouse::click::Kind::Single => {
                            Action::Click(click.position())
                        }
                        mouse::click::Kind::Double => Action::SelectWord,
                        mouse::click::Kind::Triple => Action::SelectLine,
                    };

                    // Mtrx patch (editing): a press in the selection grabs
                    // it, to drag it; only while focused, as it is drawn
                    // only then.
                    let grabs = state.focus.is_some()
                        && matches!(click.kind(), mouse::click::Kind::Single)
                        && self.content.selection_contains(click.position());

                    state.focus = Some(Focus::now());
                    state.last_click = Some(click);

                    if grabs {
                        state.drag_text = Some(DragText {
                            from: click.position(),
                            to: None,
                            moved: false,
                        });
                    } else {
                        state.drag_click = Some(click.kind());

                        shell.publish(on_edit(action));
                    }

                    shell.capture_event();
                }
                // Mtrx patch (editing).
                Update::DragText { at, over } => {
                    if let Some(drag) = &mut state.drag_text {
                        drag.moved |=
                            drag.from.distance(at) > DragText::THRESHOLD;
                        drag.to = (drag.moved && over).then_some(at);

                        shell.request_redraw();
                    }
                }
                Update::Drag(position) => {
                    shell.publish(on_edit(Action::Drag(position)));
                }
                Update::Release => {
                    state.drag_click = None;

                    // Mtrx patch (editing): the selection dropped where the
                    // pointer is; not moved, a click there.
                    if let Some(drag) = state.drag_text.take() {
                        if let Some(to) = drag.to {
                            shell.publish(on_edit(Action::Edit(
                                Edit::MoveSelection {
                                    to,
                                    copy: copies(state.modifiers),
                                },
                            )));
                        } else if !drag.moved {
                            shell.publish(on_edit(Action::Click(drag.from)));
                        }

                        shell.request_redraw();
                    }
                }
                Update::Scroll(lines) => {
                    let bounds = self.content.0.borrow().editor.bounds();

                    if bounds.height >= i32::MAX as f32 {
                        return;
                    }

                    let lines = lines + state.partial_scroll;
                    state.partial_scroll = lines.fract();

                    shell.publish(on_edit(Action::Scroll {
                        lines: lines as i32,
                    }));
                    shell.capture_event();
                }
                Update::InputMethod(update) => match update {
                    Ime::Toggle(is_open) => {
                        state.preedit =
                            is_open.then(input_method::Preedit::new);

                        shell.request_redraw();
                    }
                    Ime::Preedit { content, selection } => {
                        state.preedit = Some(input_method::Preedit {
                            content,
                            selection,
                            text_size: self.text_size,
                        });

                        shell.request_redraw();
                    }
                    Ime::Commit(text) => {
                        shell.publish(on_edit(Action::Edit(Edit::Paste(
                            Arc::new(text),
                        ))));
                    }
                    // Mtrx patch (surrounding text).
                    Ime::DeleteSurrounding { before, after } => {
                        shell.publish(on_edit(Action::Edit(
                            Edit::DeleteSurrounding { before, after },
                        )));
                    }
                },
                Update::Binding(binding) => {
                    fn apply_binding<
                        H: text::Highlighter,
                        R: text::Renderer,
                        Message,
                    >(
                        binding: Binding<Message>,
                        content: &Content<R>,
                        state: &mut State<H>,
                        on_edit: &dyn Fn(Action) -> Message,
                        clipboard: &mut dyn Clipboard,
                        shell: &mut Shell<'_, Message>,
                    ) {
                        let mut publish =
                            |action| shell.publish(on_edit(action));

                        match binding {
                            Binding::Unfocus => {
                                state.focus = None;
                                state.drag_click = None;
                                // Mtrx patch (editing).
                                state.drag_text = None;
                            }
                            Binding::Copy => {
                                if let Some(selection) = content.selection() {
                                    clipboard.write(
                                        clipboard::Kind::Standard,
                                        selection,
                                    );
                                }
                            }
                            Binding::Cut => {
                                if let Some(selection) = content.selection() {
                                    clipboard.write(
                                        clipboard::Kind::Standard,
                                        selection,
                                    );

                                    publish(Action::Edit(Edit::Delete));
                                }
                            }
                            Binding::Paste => {
                                if let Some(contents) =
                                    clipboard.read(clipboard::Kind::Standard)
                                {
                                    publish(Action::Edit(Edit::Paste(
                                        Arc::new(contents),
                                    )));
                                }
                            }
                            Binding::Move(motion) => {
                                publish(Action::Move(motion));
                            }
                            Binding::Select(motion) => {
                                publish(Action::Select(motion));
                            }
                            Binding::SelectWord => {
                                publish(Action::SelectWord);
                            }
                            Binding::SelectLine => {
                                publish(Action::SelectLine);
                            }
                            Binding::SelectAll => {
                                publish(Action::SelectAll);
                            }
                            Binding::Insert(c) => {
                                publish(Action::Edit(Edit::Insert(c)));
                            }
                            Binding::Enter => {
                                publish(Action::Edit(Edit::Enter));
                            }
                            Binding::Backspace => {
                                publish(Action::Edit(Edit::Backspace));
                            }
                            Binding::Delete => {
                                publish(Action::Edit(Edit::Delete));
                            }
                            // Mtrx patch (editing).
                            Binding::Undo => {
                                publish(Action::Edit(Edit::Undo));
                            }
                            Binding::Redo => {
                                publish(Action::Edit(Edit::Redo));
                            }
                            Binding::DeleteTo(motion) => {
                                publish(Action::Edit(Edit::DeleteTo(motion)));
                            }
                            Binding::Kill => publish(Action::Edit(Edit::Kill)),
                            Binding::Yank => publish(Action::Edit(Edit::Yank)),
                            Binding::Transpose => {
                                publish(Action::Edit(Edit::Transpose));
                            }
                            Binding::OpenLine => {
                                publish(Action::Edit(Edit::OpenLine));
                            }
                            Binding::Sequence(sequence) => {
                                for binding in sequence {
                                    apply_binding(
                                        binding, content, state, on_edit,
                                        clipboard, shell,
                                    );
                                }
                            }
                            Binding::Custom(message) => {
                                shell.publish(message);
                            }
                        }
                    }

                    if !matches!(binding, Binding::Unfocus) {
                        shell.capture_event();
                    }

                    apply_binding(
                        binding,
                        self.content,
                        state,
                        on_edit,
                        clipboard,
                        shell,
                    );

                    if let Some(focus) = &mut state.focus {
                        focus.updated_at = Instant::now();
                    }
                }
            }
        }

        let status = {
            let is_disabled = self.on_edit.is_none();
            let is_hovered = cursor.is_over(layout.bounds());

            if is_disabled {
                Status::Disabled
            } else if state.focus.is_some() {
                Status::Focused { is_hovered }
            } else if is_hovered {
                Status::Hovered
            } else {
                Status::Active
            }
        };

        if is_redraw {
            self.last_status = Some(status);

            shell.request_input_method(
                &self.input_method(state, renderer, layout),
            );
        } else if self
            .last_status
            .is_some_and(|last_status| status != last_status)
        {
            shell.request_redraw();
        }
    }

    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _defaults: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();

        let mut internal = self.content.0.borrow_mut();
        let state = tree.state.downcast_ref::<State<Highlighter>>();

        let font = self.font.unwrap_or_else(|| renderer.default_font());

        let theme_name = theme.name();

        if state
            .last_theme
            .borrow()
            .as_ref()
            .is_none_or(|last_theme| last_theme != theme_name)
        {
            state.highlighter.borrow_mut().change_line(0);
            let _ =
                state.last_theme.borrow_mut().replace(theme_name.to_owned());
        }

        internal.editor.highlight(
            font,
            state.highlighter.borrow_mut().deref_mut(),
            |highlight| (self.highlighter_format)(highlight, theme),
        );

        let style = theme
            .style(&self.class, self.last_status.unwrap_or(Status::Active));

        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: style.border,
                ..renderer::Quad::default()
            },
            style.background,
        );

        let text_bounds = bounds.shrink(self.padding);

        if internal.editor.is_empty() {
            if let Some(placeholder) = self.placeholder.clone() {
                renderer.fill_text(
                    Text {
                        content: placeholder.into_owned(),
                        bounds: text_bounds.size(),
                        size: self
                            .text_size
                            .unwrap_or_else(|| renderer.default_size()),
                        line_height: self.line_height,
                        font,
                        align_x: text::Alignment::Default,
                        align_y: alignment::Vertical::Top,
                        shaping: text::Shaping::Advanced,
                        wrapping: self.wrapping,
                    },
                    text_bounds.position(),
                    style.placeholder,
                    text_bounds,
                );
            }
        } else {
            renderer.fill_editor(
                &internal.editor,
                text_bounds.position(),
                style.value,
                text_bounds,
            );
        }

        let translation = text_bounds.position() - Point::ORIGIN;

        if let Some(focus) = state.focus.as_ref() {
            match internal.editor.selection() {
                Selection::Caret(position) if focus.is_cursor_visible() => {
                    let cursor =
                        Rectangle::new(
                            position + translation,
                            Size::new(
                                1.0,
                                self.line_height
                                    .to_absolute(self.text_size.unwrap_or_else(
                                        || renderer.default_size(),
                                    ))
                                    .into(),
                            ),
                        );

                    if let Some(clipped_cursor) =
                        text_bounds.intersection(&cursor)
                    {
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: clipped_cursor,
                                ..renderer::Quad::default()
                            },
                            style.value,
                        );
                    }
                }
                Selection::Range(ranges) => {
                    for range in ranges.into_iter().filter_map(|range| {
                        text_bounds.intersection(&(range + translation))
                    }) {
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: range,
                                ..renderer::Quad::default()
                            },
                            style.selection,
                        );
                    }
                }
                Selection::Caret(_) => {}
            }
        }

        // Mtrx patch (editing): where the dragged selection would drop.
        if let Some(DragText { to: Some(to), .. }) = state.drag_text
            && let Some(caret) = internal.editor.caret_at(to)
        {
            let caret = Rectangle::new(
                caret + translation,
                Size::new(
                    1.0,
                    self.line_height
                        .to_absolute(
                            self.text_size
                                .unwrap_or_else(|| renderer.default_size()),
                        )
                        .into(),
                ),
            );

            if let Some(caret) = text_bounds.intersection(&caret) {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: caret,
                        ..renderer::Quad::default()
                    },
                    style.value,
                );
            }
        }
    }

    fn mouse_interaction(
        &self,
        tree: &widget::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let is_disabled = self.on_edit.is_none();
        // Mtrx patch (editing).
        let state = tree.state.downcast_ref::<State<Highlighter>>();
        let over_selection = || {
            cursor.position_in(layout.bounds()).is_some_and(|position| {
                self.content.selection_contains(
                    position - Vector::new(self.padding.left, self.padding.top),
                )
            })
        };

        if cursor.is_over(layout.bounds()) {
            if is_disabled {
                mouse::Interaction::NotAllowed
            // Mtrx patch (editing): an arrow over the selection, which
            // drags, as a text view has; a copy sign with ⌥ while dragging.
            } else if state.drag_text.is_some() && copies(state.modifiers) {
                mouse::Interaction::Copy
            } else if state.drag_text.is_some()
                || (state.focus.is_some() && over_selection())
            {
                mouse::Interaction::Idle
            } else {
                mouse::Interaction::Text
            }
        } else {
            mouse::Interaction::default()
        }
    }

    fn operate(
        &mut self,
        tree: &mut widget::Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        let state = tree.state.downcast_mut::<State<Highlighter>>();

        operation.focusable(self.id.as_ref(), layout.bounds(), state);
    }
}

impl<'a, Highlighter, Message, Theme, Renderer>
    From<TextEditor<'a, Highlighter, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Highlighter: text::Highlighter,
    Message: 'a,
    Theme: Catalog + 'a,
    Renderer: text::Renderer,
{
    fn from(
        text_editor: TextEditor<'a, Highlighter, Message, Theme, Renderer>,
    ) -> Self {
        Self::new(text_editor)
    }
}

/// A binding to an action in the [`TextEditor`].
#[derive(Debug, Clone, PartialEq)]
pub enum Binding<Message> {
    /// Unfocus the [`TextEditor`].
    Unfocus,
    /// Copy the selection of the [`TextEditor`].
    Copy,
    /// Cut the selection of the [`TextEditor`].
    Cut,
    /// Paste the clipboard contents in the [`TextEditor`].
    Paste,
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
    /// Insert the given character.
    Insert(char),
    /// Break the current line.
    Enter,
    /// Delete the previous character.
    Backspace,
    /// Delete the next character.
    Delete,
    // Mtrx patch (editing).
    /// Undo the last step of the editor's history.
    Undo,
    /// Redo the last step undone.
    Redo,
    /// Delete the selection, or to where a [`Motion`] goes.
    DeleteTo(Motion),
    /// Delete to the end of the paragraph, keeping it to yank: ⌃K.
    Kill,
    /// Insert the text last killed: ⌃Y.
    Yank,
    /// Swap the characters around the cursor: ⌃T.
    Transpose,
    /// Break the line after the cursor: ⌃O.
    OpenLine,
    /// A sequence of bindings to execute.
    Sequence(Vec<Self>),
    /// Produce the given message.
    Custom(Message),
}

/// A key press.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyPress {
    /// The original key pressed without modifiers applied to it.
    ///
    /// You should use this key for combinations (e.g. Ctrl+C).
    pub key: keyboard::Key,
    /// The key pressed with modifiers applied to it.
    ///
    /// You should use this key for any single key bindings (e.g. motions).
    pub modified_key: keyboard::Key,
    /// The physical key pressed.
    ///
    /// You should use this key for layout-independent bindings.
    pub physical_key: keyboard::key::Physical,
    /// The state of the keyboard modifiers.
    pub modifiers: keyboard::Modifiers,
    /// The text produced by the key press.
    pub text: Option<SmolStr>,
    /// The current [`Status`] of the [`TextEditor`].
    pub status: Status,
}

impl<Message> Binding<Message> {
    /// Returns the default [`Binding`] for the given key press.
    pub fn from_key_press(event: KeyPress) -> Option<Self> {
        let KeyPress {
            key,
            modified_key,
            physical_key,
            modifiers,
            text,
            status,
        } = event;

        if !matches!(status, Status::Focused { .. }) {
            return None;
        }

        let combination = match key.to_latin(physical_key) {
            Some('c') if modifiers.command() => Some(Self::Copy),
            Some('x') if modifiers.command() => Some(Self::Cut),
            Some('v') if modifiers.command() && !modifiers.alt() => {
                Some(Self::Paste)
            }
            Some('a') if modifiers.command() => Some(Self::SelectAll),
            // Mtrx patch (editing): ⌘Z and ⇧⌘Z (Ctrl+Z, Ctrl+Shift+Z and
            // Ctrl+Y elsewhere).
            Some('z' | 'Z') if modifiers.command() && modifiers.shift() => {
                Some(Self::Redo)
            }
            Some('z') if modifiers.command() => Some(Self::Undo),
            Some('y') if modifiers.command() && !cfg!(target_os = "macos") => {
                Some(Self::Redo)
            }
            _ => None,
        };

        if let Some(binding) = combination {
            return Some(binding);
        }

        // Mtrx patch (editing): instead of `convert_macos_shortcut`, whose
        // keys came with a control character as their text, which typed
        // nothing and so bound nothing (only ⌃H worked).
        #[cfg(target_os = "macos")]
        if let Some(binding) = emacs(&key, physical_key, modifiers) {
            return Some(binding);
        }

        match modified_key.as_ref() {
            keyboard::Key::Named(key::Named::Enter) => Some(Self::Enter),
            // Mtrx patch (editing): ⌥⌫ and ⌥⌦ delete a word (Ctrl with
            // them elsewhere), ⌘⌫ and ⌘⌦ to the edge of the line.
            keyboard::Key::Named(key::Named::Backspace) => {
                Some(if modifiers.macos_command() {
                    Self::DeleteTo(Motion::Home)
                } else if modifiers.jump() {
                    Self::DeleteTo(Motion::WordLeft)
                } else {
                    Self::Backspace
                })
            }
            keyboard::Key::Named(key::Named::Delete)
                if text.is_none() || text.as_deref() == Some("\u{7f}") =>
            {
                Some(if modifiers.macos_command() {
                    Self::DeleteTo(Motion::End)
                } else if modifiers.jump() {
                    Self::DeleteTo(Motion::WordRight)
                } else {
                    Self::Delete
                })
            }
            keyboard::Key::Named(key::Named::Escape) => Some(Self::Unfocus),
            _ => {
                // Mtrx patch: not the text of a shortcut.
                if let Some(text) = text.filter(|_| !is_shortcut(modifiers)) {
                    let c = text.chars().find(|c| !c.is_control())?;

                    Some(Self::Insert(c))
                } else if let keyboard::Key::Named(named_key) = key.as_ref() {
                    let motion = motion(named_key)?;

                    let motion = if modifiers.macos_command() {
                        match motion {
                            Motion::Left => Motion::Home,
                            Motion::Right => Motion::End,
                            // Mtrx patch (editing): ⌘↑ and ⌘↓.
                            Motion::Up => Motion::DocumentStart,
                            Motion::Down => Motion::DocumentEnd,
                            _ => motion,
                        }
                    } else {
                        motion
                    };

                    let motion = if modifiers.jump() {
                        motion.widen()
                    } else {
                        motion
                    };

                    Some(if modifiers.shift() {
                        Self::Select(motion)
                    } else {
                        Self::Move(motion)
                    })
                } else {
                    None
                }
            }
        }
    }
}

enum Update<Message> {
    Click(mouse::Click),
    Drag(Point),
    // Mtrx patch (editing).
    DragText { at: Point, over: bool },
    Release,
    Scroll(f32),
    InputMethod(Ime),
    Binding(Binding<Message>),
}

enum Ime {
    Toggle(bool),
    Preedit {
        content: String,
        selection: Option<ops::Range<usize>>,
    },
    Commit(String),
    // Mtrx patch (surrounding text).
    DeleteSurrounding {
        before: usize,
        after: usize,
    },
}

impl<Message> Update<Message> {
    fn from_event<H: Highlighter>(
        event: &Event,
        state: &State<H>,
        bounds: Rectangle,
        padding: Padding,
        cursor: mouse::Cursor,
        key_binding: Option<&dyn Fn(KeyPress) -> Option<Binding<Message>>>,
    ) -> Option<Self> {
        let binding = |binding| Some(Update::Binding(binding));

        match event {
            Event::Mouse(event) => match event {
                mouse::Event::ButtonPressed(mouse::Button::Left) => {
                    if let Some(cursor_position) = cursor.position_in(bounds) {
                        let cursor_position = cursor_position
                            - Vector::new(padding.left, padding.top);

                        let click = mouse::Click::new(
                            cursor_position,
                            mouse::Button::Left,
                            state.last_click,
                        );

                        Some(Update::Click(click))
                    } else if state.focus.is_some() {
                        binding(Binding::Unfocus)
                    } else {
                        None
                    }
                }
                mouse::Event::ButtonReleased(mouse::Button::Left) => {
                    Some(Update::Release)
                }
                // Mtrx patch (editing): the selection dragged, also beyond
                // the editor.
                mouse::Event::CursorMoved { .. }
                    if state.drag_text.is_some() =>
                {
                    let position = cursor.position()?;

                    Some(Update::DragText {
                        at: position
                            - (bounds.position() - Point::ORIGIN)
                            - Vector::new(padding.left, padding.top),
                        over: bounds.contains(position),
                    })
                }
                mouse::Event::CursorMoved { .. } => match state.drag_click {
                    Some(mouse::click::Kind::Single) => {
                        let cursor_position = cursor.position_in(bounds)?
                            - Vector::new(padding.left, padding.top);

                        Some(Update::Drag(cursor_position))
                    }
                    _ => None,
                },
                mouse::Event::WheelScrolled { delta }
                    if cursor.is_over(bounds) =>
                {
                    Some(Update::Scroll(match delta {
                        mouse::ScrollDelta::Lines { y, .. } => {
                            if y.abs() > 0.0 {
                                y.signum() * -(y.abs() * 4.0).max(1.0)
                            } else {
                                0.0
                            }
                        }
                        mouse::ScrollDelta::Pixels { y, .. } => -y / 4.0,
                    }))
                }
                _ => None,
            },
            Event::InputMethod(event) => match event {
                input_method::Event::Opened | input_method::Event::Closed => {
                    Some(Update::InputMethod(Ime::Toggle(matches!(
                        event,
                        input_method::Event::Opened
                    ))))
                }
                input_method::Event::Preedit(content, selection)
                    if state.focus.is_some() =>
                {
                    Some(Update::InputMethod(Ime::Preedit {
                        content: content.clone(),
                        selection: selection.clone(),
                    }))
                }
                input_method::Event::Commit(content)
                    if state.focus.is_some() =>
                {
                    Some(Update::InputMethod(Ime::Commit(content.clone())))
                }
                // Mtrx patch (surrounding text).
                input_method::Event::DeleteSurrounding {
                    before_bytes,
                    after_bytes,
                } if state.focus.is_some() => {
                    Some(Update::InputMethod(Ime::DeleteSurrounding {
                        before: *before_bytes,
                        after: *after_bytes,
                    }))
                }
                _ => None,
            },
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modified_key,
                physical_key,
                modifiers,
                text,
                ..
            }) => {
                let status = if state.focus.is_some() {
                    Status::Focused {
                        is_hovered: cursor.is_over(bounds),
                    }
                } else {
                    Status::Active
                };

                let key_press = KeyPress {
                    key: key.clone(),
                    modified_key: modified_key.clone(),
                    physical_key: *physical_key,
                    modifiers: *modifiers,
                    text: text.clone(),
                    status,
                };

                if let Some(key_binding) = key_binding {
                    key_binding(key_press)
                } else {
                    Binding::from_key_press(key_press)
                }
                // Mtrx patch: a custom binding (Enter sends, say) only while
                // focused, as the default bindings; the bindings are asked
                // also without the focus, and a custom one was published.
                .filter(|binding| {
                    state.focus.is_some()
                        || !matches!(binding, Binding::Custom(_))
                })
                .map(Self::Binding)
            }
            // Mtrx patch (menu actions): as the key press it stands for, so
            // that the key binding decides (the app's too: ⌘V may paste a
            // picture).
            Event::StandardAction(action) => Self::from_event(
                &Event::Keyboard(action.key_press()),
                state,
                bounds,
                padding,
                cursor,
                key_binding,
            ),
            _ => None,
        }
    }
}

fn motion(key: key::Named) -> Option<Motion> {
    match key {
        key::Named::ArrowLeft => Some(Motion::Left),
        key::Named::ArrowRight => Some(Motion::Right),
        key::Named::ArrowUp => Some(Motion::Up),
        key::Named::ArrowDown => Some(Motion::Down),
        key::Named::Home => Some(Motion::Home),
        key::Named::End => Some(Motion::End),
        key::Named::PageUp => Some(Motion::PageUp),
        key::Named::PageDown => Some(Motion::PageDown),
        _ => None,
    }
}

/// The possible status of a [`TextEditor`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The [`TextEditor`] can be interacted with.
    Active,
    /// The [`TextEditor`] is being hovered.
    Hovered,
    /// The [`TextEditor`] is focused.
    Focused {
        /// Whether the [`TextEditor`] is hovered, while focused.
        is_hovered: bool,
    },
    /// The [`TextEditor`] cannot be interacted with.
    Disabled,
}

/// The appearance of a text input.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    /// The [`Background`] of the text input.
    pub background: Background,
    /// The [`Border`] of the text input.
    pub border: Border,
    /// The [`Color`] of the placeholder of the text input.
    pub placeholder: Color,
    /// The [`Color`] of the value of the text input.
    pub value: Color,
    /// The [`Color`] of the selection of the text input.
    pub selection: Color,
}

/// The theme catalog of a [`TextEditor`].
pub trait Catalog: theme::Base {
    /// The item class of the [`Catalog`].
    type Class<'a>;

    /// The default class produced by the [`Catalog`].
    fn default<'a>() -> Self::Class<'a>;

    /// The [`Style`] of a class with the given status.
    fn style(&self, class: &Self::Class<'_>, status: Status) -> Style;
}

/// A styling function for a [`TextEditor`].
pub type StyleFn<'a, Theme> = Box<dyn Fn(&Theme, Status) -> Style + 'a>;

impl Catalog for Theme {
    type Class<'a> = StyleFn<'a, Self>;

    fn default<'a>() -> Self::Class<'a> {
        Box::new(default)
    }

    fn style(&self, class: &Self::Class<'_>, status: Status) -> Style {
        class(self, status)
    }
}

/// The default style of a [`TextEditor`].
pub fn default(theme: &Theme, status: Status) -> Style {
    let palette = theme.extended_palette();

    let active = Style {
        background: Background::Color(palette.background.base.color),
        border: Border {
            radius: 2.0.into(),
            width: 1.0,
            color: palette.background.strong.color,
        },
        placeholder: palette.secondary.base.color,
        value: palette.background.base.text,
        selection: palette.primary.weak.color,
    };

    match status {
        Status::Active => active,
        Status::Hovered => Style {
            border: Border {
                color: palette.background.base.text,
                ..active.border
            },
            ..active
        },
        Status::Focused { .. } => Style {
            border: Border {
                color: palette.primary.strong.color,
                ..active.border
            },
            ..active
        },
        Status::Disabled => Style {
            background: Background::Color(palette.background.weak.color),
            value: active.placeholder,
            placeholder: palette.background.strongest.color,
            ..active
        },
    }
}

/// Mtrx patch: whether a key pressed with `modifiers` is a shortcut, whose
/// text is not typed: macOS gives the text of ⌘1 or ⌘K too. Elsewhere Ctrl,
/// but not with Alt: Ctrl+Alt is AltGr on Windows, which types.
pub(crate) fn is_shortcut(modifiers: keyboard::Modifiers) -> bool {
    if cfg!(target_os = "macos") {
        modifiers.logo()
    } else {
        modifiers.control() && !modifiers.alt()
    }
}

/// Mtrx patch (editing): the Emacs keys of macOS's text system, as its
/// standard key bindings have them.
#[cfg(target_os = "macos")]
fn emacs<Message>(
    key: &keyboard::Key,
    physical_key: key::Physical,
    modifiers: keyboard::Modifiers,
) -> Option<Binding<Message>> {
    if modifiers != keyboard::Modifiers::CTRL {
        return None;
    }

    Some(match key.to_latin(physical_key)? {
        'a' => Binding::Move(Motion::ParagraphStart),
        'e' => Binding::Move(Motion::ParagraphEnd),
        'b' => Binding::Move(Motion::Left),
        'f' => Binding::Move(Motion::Right),
        'p' => Binding::Move(Motion::Up),
        'n' => Binding::Move(Motion::Down),
        'h' => Binding::Backspace,
        'd' => Binding::Delete,
        'k' => Binding::Kill,
        'y' => Binding::Yank,
        't' => Binding::Transpose,
        'o' => Binding::OpenLine,
        _ => return None,
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn convert_macos_shortcut(
    key: &keyboard::Key,
    modifiers: keyboard::Modifiers,
) -> Option<keyboard::Key> {
    if modifiers != keyboard::Modifiers::CTRL {
        return None;
    }

    let key = match key.as_ref() {
        keyboard::Key::Character("b") => key::Named::ArrowLeft,
        keyboard::Key::Character("f") => key::Named::ArrowRight,
        keyboard::Key::Character("a") => key::Named::Home,
        keyboard::Key::Character("e") => key::Named::End,
        keyboard::Key::Character("h") => key::Named::Backspace,
        keyboard::Key::Character("d") => key::Named::Delete,
        _ => return None,
    };

    Some(keyboard::Key::Named(key))
}
