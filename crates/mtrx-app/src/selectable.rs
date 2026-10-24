//! Rich text whose text can be selected, like iced's `rich_text` (whose
//! layout and drawing it follows) plus hit-testing and highlighting through
//! the paragraph's cosmic-text layout. In the timeline the selection lives
//! in the room ([`crate::selection`]): the widget reports gestures and draws
//! the part it is given, and the find bar's matches ([`crate::find`]) the
//! same way. Elsewhere ([`selectable_text`]) it keeps its own
//! selection and copies it with ⌘C.

use std::{any::Any, ops::Range, rc::Rc};

use iced::{
    Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Renderer, Size, Task, Theme,
    Vector,
    advanced::{
        Clipboard, Layout, Shell, Widget, clipboard, layout, mouse, overlay, renderer,
        text::{self as core_text, Paragraph as _, Span},
        widget::{
            Id, Operation, operate,
            operation::Outcome,
            text as core_widget_text,
            tree::{self, Tree},
        },
    },
    alignment, keyboard,
    widget::text::{LineHeight, Shaping, Wrapping},
};
use iced_graphics::text::{
    Paragraph,
    cosmic_text::{Buffer, Cursor},
};
use unicode_segmentation::UnicodeSegmentation;

use crate::{find, link_status, selection::Gesture, style};

/// How far the cursor may move between pressing and releasing a link for it
/// to count as a click rather than the start of a selection.
pub const CLICK_SLOP: f32 = 4.0;

/// The id of the unit holding the end of the selection, for [`line_step`].
const FOCUS: &str = "selection-focus";

pub struct Selectable<'a, Message> {
    spans: Vec<Span<'static, String, Font>>,
    /// The bytes of `spans` that are its text, when spans after them are
    /// only shown (see [`Selectable::after`]).
    text_len: Option<usize>,
    /// Its index among the message's units.
    unit: usize,
    size: Option<Pixels>,
    /// The selected part of its text, in bytes.
    selected: Option<Range<usize>>,
    /// Matches of the find bar in its text, and whether each is the current
    /// one.
    found: Vec<(Range<usize>, bool)>,
    /// The selection ends in it.
    focus: bool,
    dragging: bool,
    on_link: Option<fn(String) -> Message>,
    on_link_menu: Option<Rc<dyn Fn(String) -> Message + 'a>>,
    on_gesture: Option<Rc<dyn Fn(Gesture) -> Message + 'a>>,
    /// Keeps its own selection, not the room's.
    own: bool,
    wrapping: Wrapping,
    style: Option<fn(&Theme) -> core_widget_text::Style>,
}

/// Text with its own selection, copied with ⌘C, e.g. in info cards.
pub fn selectable_text<'a, Message>(content: impl Into<String>) -> Selectable<'a, Message> {
    Selectable {
        own: true,
        ..selectable(vec![Span::new(content.into())], 0)
    }
}

/// Unit `unit` of a message, showing `spans`.
pub fn selectable<'a, Message>(
    spans: Vec<Span<'static, String, Font>>,
    unit: usize,
) -> Selectable<'a, Message> {
    Selectable {
        spans,
        text_len: None,
        unit,
        size: None,
        selected: None,
        found: Vec::new(),
        focus: false,
        dragging: false,
        on_link: None,
        on_link_menu: None,
        on_gesture: None,
        own: false,
        wrapping: Wrapping::default(),
        style: None,
    }
}

impl<'a, Message> Selectable<'a, Message> {
    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.size = Some(size.into());
        self
    }

    /// The font of all its text.
    pub fn font(mut self, font: Font) -> Self {
        for span in &mut self.spans {
            span.font = Some(font);
        }
        self
    }

    /// Shown after its text, without being part of it: never selected or
    /// copied, e.g. the "(edited)" of a message.
    pub fn after(mut self, span: Span<'static, String, Font>) -> Self {
        let len = self.spans.iter().map(|s| s.text.len()).sum();
        self.text_len.get_or_insert(len);
        self.spans.push(span);
        self
    }

    pub fn wrapping(mut self, wrapping: Wrapping) -> Self {
        self.wrapping = wrapping;
        self
    }

    /// The colour of text without its own.
    pub fn style(mut self, style: fn(&Theme) -> core_widget_text::Style) -> Self {
        self.style = Some(style);
        self
    }

    /// Sent when a link is clicked (pressed and released without moving).
    pub fn on_link(mut self, on_link: fn(String) -> Message) -> Self {
        self.on_link = Some(on_link);
        self
    }

    /// Sent when a link is right-clicked, for its menu.
    pub fn on_link_menu(mut self, on_link_menu: Rc<dyn Fn(String) -> Message + 'a>) -> Self {
        self.on_link_menu = Some(on_link_menu);
        self
    }

    pub fn selected(mut self, range: Option<Range<usize>>) -> Self {
        self.selected = range;
        self
    }

    pub fn found(mut self, found: Vec<(Range<usize>, bool)>) -> Self {
        self.found = found;
        self
    }

    /// The selection ends in it, so Shift with ↑ or ↓ moves from it.
    pub fn focus(mut self, focus: bool) -> Self {
        self.focus = focus;
        self
    }

    /// A selection is being dragged: report where the cursor is.
    pub fn dragging(mut self, dragging: bool) -> Self {
        self.dragging = dragging;
        self
    }

    pub fn on_gesture(mut self, on_gesture: Rc<dyn Fn(Gesture) -> Message + 'a>) -> Self {
        self.on_gesture = Some(on_gesture);
        self
    }
}

#[derive(Default)]
struct State {
    spans: Vec<Span<'static, String, Font>>,
    text_len: Option<usize>,
    paragraph: Paragraph,
    /// Its text (the spans' text but what is only shown after it), and where
    /// each line of the spans' text starts.
    text: String,
    line_starts: Vec<usize>,
    last_click: Option<mouse::Click>,
    modifiers: keyboard::Modifiers,
    /// The link span pressed, and where.
    link_pressed: Option<(usize, Point)>,
    hovered_link: Option<usize>,
    /// The hovered link's address in the window's corner.
    link_status: link_status::Status,
    /// With its own selection: where it starts and ends, and whether it is
    /// being dragged.
    own: Option<(usize, usize)>,
    own_dragging: bool,
}

impl State {
    fn set_spans(&mut self, spans: &[Span<'static, String, Font>], text_len: Option<usize>) {
        self.spans = spans.to_vec();
        self.text_len = text_len;
        let mut shown: String = spans.iter().map(|s| s.text.as_ref()).collect();
        self.line_starts = std::iter::once(0)
            .chain(shown.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        if let Some(len) = text_len {
            shown.truncate(len);
        }
        self.text = shown;
    }

    fn buffer(&self) -> &Buffer {
        self.paragraph.buffer()
    }

    /// The text offset nearest to `point` (relative to the text).
    fn offset_at(&self, point: Point, size: Size) -> usize {
        let x = point.x.clamp(0.0, size.width);
        let y = point.y.clamp(0.0, (size.height - 0.5).max(0.0));
        match self.buffer().hit(x, y) {
            Some(cursor) => (self.line_starts.get(cursor.line).copied().unwrap_or(0)
                + cursor.index)
                .min(self.text.len()),
            None if point.y <= 0.0 => 0,
            None => self.text.len(),
        }
    }

    /// The range a press at `at` (`position` in the window) selects: a
    /// point, or the word or all for a double or triple click.
    fn press(&mut self, at: Point, position: Point, size: Size) -> (usize, usize) {
        let click = mouse::Click::new(position, mouse::Button::Left, self.last_click);
        self.last_click = Some(click);
        let offset = self.offset_at(at, size);
        match click.kind() {
            mouse::click::Kind::Single => (offset, offset),
            mouse::click::Kind::Double => {
                let word = self.word_at(offset);
                (word.start, word.end)
            }
            mouse::click::Kind::Triple => (0, self.text.len()),
        }
    }

    /// Its own selection, if not empty.
    fn own_range(&self) -> Option<Range<usize>> {
        let (anchor, focus) = self.own?;
        (anchor != focus).then(|| anchor.min(focus)..anchor.max(focus))
    }

    /// The word (or run of spaces or punctuation) around `offset`.
    fn word_at(&self, offset: usize) -> Range<usize> {
        self.text
            .split_word_bound_indices()
            .map(|(start, word)| start..start + word.len())
            .find(|word| word.contains(&offset))
            .unwrap_or(offset..offset)
    }

    /// The offset a line above (`up`) or below `offset`, nearest to where
    /// it is across; `None` past the first or last line.
    fn line_step(&self, offset: usize, up: bool, size: Size) -> Option<usize> {
        let runs: Vec<_> = self.buffer().layout_runs().collect();
        let (at, x) = runs.iter().enumerate().find_map(|(i, run)| {
            let start = self.line_starts.get(run.line_i).copied().unwrap_or(0);
            let local = offset.checked_sub(start)?;
            let (from, to) = match (run.glyphs.first(), run.glyphs.last()) {
                (Some(first), Some(last)) => (first.start, last.end),
                // An empty line.
                _ => (0, 0),
            };
            (from..=to).contains(&local).then(|| {
                let x = (run.glyphs.iter())
                    .find(|glyph| glyph.start >= local)
                    .map_or(run.line_w, |glyph| glyph.x);
                (i, x)
            })
        })?;
        let run = runs.get(if up { at.checked_sub(1)? } else { at + 1 })?;
        Some(self.offset_at(Point::new(x, run.line_top + run.line_height / 2.0), size))
    }

    /// The start or end (`forward`) of the line holding `offset`. An offset
    /// where one line ends and the next starts (a word broken across them)
    /// is the end of the first going forward and the start of the second
    /// going back, so pressing again stays.
    fn line_edge(&self, offset: usize, forward: bool) -> Option<usize> {
        let mut lines = self.buffer().layout_runs().filter_map(|run| {
            let start = self.line_starts.get(run.line_i).copied().unwrap_or(0);
            // In either direction of writing.
            let from = run.glyphs.iter().map(|g| g.start).min().unwrap_or(0);
            let to = run.glyphs.iter().map(|g| g.end).max().unwrap_or(0);
            let (from, to) = (start + from, start + to);
            (from..=to).contains(&offset).then_some((from, to))
        });
        if forward {
            lines.next().map(|(_, to)| to)
        } else {
            lines.last().map(|(from, _)| from)
        }
    }

    /// Rectangles covering `range`, relative to the text.
    fn highlight(&self, range: &Range<usize>, font_size: f32) -> Vec<Rectangle> {
        let runs: Vec<_> = self.buffer().layout_runs().collect();
        let mut rects = Vec::new();
        for (i, run) in runs.iter().enumerate() {
            let start = self.line_starts.get(run.line_i).copied().unwrap_or(0);
            let len = run.text.len();
            if range.end <= start || range.start > start + len {
                continue;
            }
            let from = range.start.saturating_sub(start).min(len);
            let to = (range.end - start).min(len);
            let line_ends = runs.get(i + 1).is_none_or(|next| next.line_i != run.line_i);
            // The line break is selected too: show a little of it.
            let newline = line_ends && range.end > start + len && start + len < self.text.len();
            let Some((x, width)) = run
                .highlight(Cursor::new(run.line_i, from), Cursor::new(run.line_i, to))
                .or_else(|| newline.then_some((run.line_w, 0.0)))
            else {
                continue;
            };
            let width = if newline {
                width + font_size * 0.3
            } else {
                width
            };
            rects.push(Rectangle::new(
                Point::new(x, run.line_top),
                Size::new(width, run.line_height),
            ));
        }
        rects
    }
}

impl<Message> Widget<Message, Theme, Renderer> for Selectable<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Shrink)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        use iced::advanced::text::Renderer as _;
        let state = tree.state.downcast_mut::<State>();
        layout::sized(limits, Length::Shrink, Length::Shrink, |limits| {
            let text = |spans| core_text::Text {
                content: spans,
                bounds: limits.max(),
                size: self.size.unwrap_or_else(|| renderer.default_size()),
                line_height: LineHeight::default(),
                font: renderer.default_font(),
                align_x: core_text::Alignment::Default,
                align_y: alignment::Vertical::Top,
                shaping: Shaping::Advanced,
                wrapping: self.wrapping,
            };
            if state.spans != self.spans || state.text_len != self.text_len {
                state.paragraph = Paragraph::with_spans(text(self.spans.as_slice()));
                state.set_spans(&self.spans, self.text_len);
            } else {
                match state.paragraph.compare(text(&[]).with_content(())) {
                    core_text::Difference::None => {}
                    core_text::Difference::Bounds => state.paragraph.resize(limits.max()),
                    core_text::Difference::Shape => {
                        state.paragraph = Paragraph::with_spans(text(self.spans.as_slice()));
                    }
                }
            }
            state.paragraph.min_bounds()
        })
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        defaults: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        use iced::advanced::{Renderer as _, text::Renderer as _};
        let bounds = layout.bounds();
        if !bounds.intersects(viewport) {
            return;
        }
        let state = tree.state.downcast_ref::<State>();
        let translation = bounds.position() - Point::ORIGIN;
        let size = self.size.unwrap_or_else(|| renderer.default_size());

        for (range, current) in &self.found {
            let (color, border) = style::found(theme, *current);
            for rect in state.highlight(range, size.0) {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: rect + translation,
                        border,
                        ..Default::default()
                    },
                    color,
                );
            }
        }
        let selected = if self.own {
            state.own_range()
        } else {
            self.selected.clone()
        };
        if let Some(range) = &selected {
            for rect in state.highlight(range, size.0) {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: rect + translation,
                        ..Default::default()
                    },
                    style::selection(theme),
                );
            }
        }

        // Backgrounds, underlines and strikethroughs, as `rich_text` does.
        for (index, span) in self.spans.iter().enumerate() {
            let hovered_link = span.link.is_some() && state.hovered_link == Some(index);
            if span.highlight.is_none() && !span.underline && !span.strikethrough && !hovered_link {
                continue;
            }
            let regions = state.paragraph.span_bounds(index);
            if let Some(highlight) = span.highlight {
                for region in &regions {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: *region + translation,
                            border: highlight.border,
                            ..Default::default()
                        },
                        highlight.background,
                    );
                }
            }
            let span_size = span.size.unwrap_or(size);
            let line_height = span.line_height.unwrap_or_default().to_absolute(span_size);
            let color: Color = span.color.unwrap_or(defaults.text_color);
            let baseline =
                translation + Vector::new(0.0, span_size.0 + (line_height.0 - span_size.0) / 2.0);
            let mut line = |offset: f32| {
                for region in &regions {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: Rectangle::new(
                                region.position() + baseline - Vector::new(0.0, offset),
                                Size::new(region.width, 1.0),
                            ),
                            ..Default::default()
                        },
                        color,
                    );
                }
            };
            if span.underline || hovered_link {
                line(span_size.0 * 0.08);
            }
            if span.strikethrough {
                line(span_size.0 / 2.0);
            }
        }

        core_widget_text::draw(
            renderer,
            defaults,
            bounds,
            &state.paragraph,
            self.style
                .map_or_else(Default::default, |style| style(theme)),
            viewport,
        );
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();
        if self.own {
            update_own(state, event, bounds, cursor, clipboard, shell);
            return;
        }
        let hovered_link = cursor
            .position_in(bounds)
            .and_then(|at| state.paragraph.hit_span(at))
            .filter(|&span| self.spans.get(span).is_some_and(|s| s.link.is_some()));
        if hovered_link != state.hovered_link {
            state.hovered_link = hovered_link;
            shell.request_redraw();
        }
        match event {
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.modifiers = *modifiers;
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(at) = cursor.position_in(bounds) else {
                    return;
                };
                let position = cursor.position().unwrap_or_default();
                if let Some(span) = hovered_link {
                    state.link_pressed = Some((span, position));
                }
                let Some(on_gesture) = &self.on_gesture else {
                    if hovered_link.is_some() {
                        shell.capture_event();
                    }
                    return;
                };
                let (from, to) = state.press(at, position, bounds.size());
                shell.publish(on_gesture(Gesture::Press {
                    unit: self.unit,
                    from,
                    to,
                    extend: state.modifiers.shift(),
                }));
                shell.capture_event();
            }
            // The link's menu rather than whatever the text is in offers.
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                if let Some(on_link_menu) = &self.on_link_menu
                    && let Some(link) = hovered_link.and_then(|span| self.spans[span].link.clone())
                {
                    shell.publish(on_link_menu(link));
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                // The cursor, not the event: inside a scrollable, only the
                // cursor is moved into the content's coordinates.
                let Some(position) = cursor.land().position() else {
                    return;
                };
                if let Some((_, pressed)) = state.link_pressed
                    && pressed.distance(position) > CLICK_SLOP
                {
                    state.link_pressed = None;
                }
                // Over its lines, also left or right of the text.
                let beside = position.y >= bounds.y && position.y < bounds.y + bounds.height;
                if self.dragging
                    && beside
                    && let Some(on_gesture) = &self.on_gesture
                {
                    let offset = state.offset_at(position - translation(bounds), bounds.size());
                    shell.publish(on_gesture(Gesture::Drag {
                        unit: self.unit,
                        offset,
                    }));
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let Some((span, _)) = state.link_pressed.take()
                    && hovered_link == Some(span)
                    && let Some(on_link) = self.on_link
                    && let Some(link) = self.spans.get(span).and_then(|s| s.link.clone())
                {
                    shell.publish(on_link(link));
                }
            }
            _ => {}
        }
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        use iced::advanced::text::Renderer as _;
        if let Some((range, _)) = self.found.iter().find(|(_, current)| *current) {
            let state = tree.state.downcast_ref::<State>();
            let size = self.size.unwrap_or_else(|| renderer.default_size());
            let rects = state.highlight(range, size.0).into_iter();
            if let Some(rect) = rects.reduce(|a, b| a.union(&b)) {
                let bounds = rect + translation(layout.bounds());
                operation.custom(Some(&Id::new(find::CURRENT)), bounds, &mut ());
            }
        }
        if self.focus {
            let state: &mut dyn Any = tree.state.downcast_mut::<State>();
            operation.custom(Some(&Id::new(FOCUS)), layout.bounds(), state);
        }
    }

    /// The address of the hovered link, if it can be clicked.
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        _layout: Layout<'b>,
        _renderer: &Renderer,
        _viewport: &Rectangle,
        _translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let state = tree.state.downcast_mut::<State>();
        let url = state
            .hovered_link
            .filter(|_| self.on_link.is_some())
            .and_then(|span| self.spans.get(span)?.link.as_deref())?;
        Some(link_status::overlay(url, &mut state.link_status))
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        if state.hovered_link.is_some() && self.on_link.is_some() {
            mouse::Interaction::Pointer
        } else if (self.on_gesture.is_some() || self.own)
            && (self.dragging || state.own_dragging || cursor.is_over(layout.bounds()))
        {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::None
        }
    }
}

/// Selecting by itself: pressing, dragging, and ⌘C; a press elsewhere
/// clears the selection.
fn update_own<Message>(
    state: &mut State,
    event: &Event,
    bounds: Rectangle,
    cursor: mouse::Cursor,
    clipboard: &mut dyn Clipboard,
    shell: &mut Shell<'_, Message>,
) {
    match event {
        Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
            state.modifiers = *modifiers;
        }
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
            let Some(at) = cursor.position_in(bounds) else {
                if state.own.take().is_some() {
                    shell.request_redraw();
                }
                return;
            };
            let position = cursor.position().unwrap_or_default();
            let (from, to) = state.press(at, position, bounds.size());
            state.own = match state.own {
                Some((anchor, _)) if state.modifiers.shift() => Some((anchor, to)),
                _ => Some((from, to)),
            };
            state.own_dragging = true;
            shell.request_redraw();
            shell.capture_event();
        }
        Event::Mouse(mouse::Event::CursorMoved { .. }) if state.own_dragging => {
            if let Some(position) = cursor.land().position()
                && let Some((anchor, _)) = state.own
            {
                let focus = state.offset_at(position - translation(bounds), bounds.size());
                state.own = Some((anchor, focus));
                shell.request_redraw();
            }
        }
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
            state.own_dragging = false;
        }
        Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. })
            if modifiers.command() && matches!(key.as_ref(), keyboard::Key::Character("c")) =>
        {
            if let Some(range) = state.own_range() {
                clipboard.write(clipboard::Kind::Standard, state.text[range].to_owned());
                shell.capture_event();
            }
        }
        _ => {}
    }
}

/// Where the end of the selection, at `offset` in the unit holding it,
/// moves a line up or down: `Some(None)` past the unit's first or last line,
/// `None` if no text unit holds it (e.g. in a typeset message).
pub fn line_step(offset: usize, up: bool) -> Task<Option<Option<usize>>> {
    operate(AtFocus::new(move |state, size| {
        state.line_step(offset, up, size)
    }))
}

/// Where the end of the selection, at `offset` in the unit holding it, is
/// at the start or end (`forward`) of its line; `None` if no text unit holds
/// it.
pub fn line_edge(offset: usize, forward: bool) -> Task<Option<usize>> {
    operate(AtFocus::new(move |state, _| {
        state.line_edge(offset, forward)
    }))
    .map(Option::flatten)
}

/// What `find` makes of the layout of the unit holding the end of the
/// selection: `None` if no text unit holds it.
struct AtFocus<T, F> {
    find: F,
    found: Option<T>,
}

impl<T, F: Fn(&State, Size) -> T> AtFocus<T, F> {
    fn new(find: F) -> Self {
        Self { find, found: None }
    }
}

impl<T: Clone + Send + 'static, F: Fn(&State, Size) -> T + Send + 'static> Operation<Option<T>>
    for AtFocus<T, F>
{
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Option<T>>)) {
        operate(self);
    }

    fn custom(&mut self, id: Option<&Id>, bounds: Rectangle, state: &mut dyn Any) {
        if id == Some(&Id::new(FOCUS))
            && let Some(state) = state.downcast_ref::<State>()
        {
            self.found = Some((self.find)(state, bounds.size()));
        }
    }

    fn finish(&self) -> Outcome<Option<T>> {
        Outcome::Some(self.found.clone())
    }
}

fn translation(bounds: Rectangle) -> Vector {
    bounds.position() - Point::ORIGIN
}

impl<'a, Message: 'a> From<Selectable<'a, Message>> for Element<'a, Message> {
    fn from(selectable: Selectable<'a, Message>) -> Self {
        Element::new(selectable)
    }
}

#[cfg(test)]
mod tests {
    use iced::{event::Status, keyboard::Key};
    use iced_test::simulator::{Simulator, press_key};

    use super::*;
    use crate::fonts;

    /// ⌘C, as a key press.
    fn command_c() -> Event {
        let Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            modified_key,
            physical_key,
            location,
            text,
            repeat,
            ..
        }) = press_key(Key::Character("c".into()), None)
        else {
            unreachable!()
        };
        Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            modified_key,
            physical_key,
            location,
            modifiers: keyboard::Modifiers::COMMAND,
            text,
            repeat,
        })
    }

    #[test]
    fn own_selection_is_dragged_and_copied() {
        let view: Element<'_, ()> = selectable_text("Ice sheet").into();
        let mut ui = Simulator::with_size(fonts::settings(), Size::new(400.0, 100.0), view);
        // Nothing selected: ⌘C is left to others.
        assert_eq!(ui.simulate([command_c()]), [Status::Ignored]);
        ui.point_at((2.0, 8.0));
        let _ = ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(
            mouse::Button::Left,
        ))]);
        ui.point_at((300.0, 8.0));
        let _ = ui.simulate([
            Event::Mouse(mouse::Event::CursorMoved {
                position: Point::new(300.0, 8.0),
            }),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ]);
        assert_eq!(ui.simulate([command_c()]), [Status::Captured]);
        // A click elsewhere clears it.
        ui.point_at((350.0, 90.0));
        let _ = ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(
            mouse::Button::Left,
        ))]);
        assert_eq!(ui.simulate([command_c()]), [Status::Ignored]);
    }

    #[test]
    fn hovered_link_shows_its_address_in_the_corner() {
        const URL: &str = "https://example.org/data";
        let spans = vec![
            Span::new("See ".to_owned()),
            Span::new("the data".to_owned()).link(URL.to_owned()),
        ];
        let view: Element<'_, String> = selectable(spans, 0).on_link(|url| url).into();
        let window = Size::new(400.0, 300.0);
        let mut ui = Simulator::with_size(fonts::settings(), window, view);
        let move_to = |ui: &mut Simulator<'_, String>, x: f32| {
            ui.point_at((x, 8.0));
            let _ = ui.simulate([Event::Mouse(mouse::Event::CursorMoved {
                position: Point::new(x, 8.0),
            })]);
        };
        assert!(ui.find(URL).is_err(), "not before hovering");
        move_to(&mut ui, 50.0);
        let bounds = ui.find(URL).expect("shown while hovered").bounds();
        assert_eq!(bounds.x, 0.0);
        assert_eq!(bounds.y + bounds.height, window.height);
        // Over the plain text, or beside it: gone.
        move_to(&mut ui, 5.0);
        assert!(ui.find(URL).is_err());
        move_to(&mut ui, 50.0);
        move_to(&mut ui, 350.0);
        assert!(ui.find(URL).is_err());
    }

    /// Wrapped over a few lines, the last after a line break.
    const WRAPPED: &str = "Ice sheets flow towards the ocean, where they calve.\nMelt";

    fn renderer() -> Renderer {
        use iced::advanced::renderer::Headless;
        iced::futures::executor::block_on(Renderer::new(
            fonts::INTER,
            Pixels(style::TEXT_SIZE),
            None,
        ))
        .unwrap()
    }

    /// What `find` makes of [`WRAPPED`] laid out 150 wide, as the unit
    /// holding the end of the selection.
    fn at_focus<T: Clone + Send + 'static>(
        renderer: &mut Renderer,
        find: impl Fn(&State, Size) -> T + Send + 'static,
    ) -> T {
        use iced::advanced::widget::operation::black_box;
        use iced_runtime::user_interface::{Cache, UserInterface};

        let view: Element<'_, ()> = iced::widget::container(
            selectable::<()>(vec![Span::new(WRAPPED.to_owned())], 0).focus(true),
        )
        .width(150)
        .into();
        let mut ui =
            UserInterface::build(view, Size::new(150.0, 400.0), Cache::default(), renderer);
        let mut operation = AtFocus::new(find);
        ui.operate(renderer, &mut black_box(&mut operation));
        operation.found.expect("the focused unit reports itself")
    }

    #[test]
    fn shift_up_and_down_move_by_line() {
        let mut renderer = renderer();
        let mut step = |offset, up| {
            at_focus(&mut renderer, move |state, size| {
                state.line_step(offset, up, size)
            })
        };
        let text = WRAPPED;
        assert_eq!(step(2, true), None);
        let below = step(2, false).unwrap();
        assert!(below > 2 && below < text.find('\n').unwrap(), "{below}");
        assert_eq!(step(below, true), Some(2));
        let last = text.len() - 1;
        assert_eq!(step(last, false), None);
        assert!(step(last, true).unwrap() < text.find('\n').unwrap());
    }

    #[test]
    fn shift_command_left_and_right_go_to_the_line_edges() {
        let mut renderer = renderer();
        let mut edge = |offset, forward| {
            at_focus(&mut renderer, move |state, _| {
                state.line_edge(offset, forward)
            })
            .unwrap()
        };
        let text = WRAPPED;
        let first_end = edge(2, true);
        assert!(
            first_end > 2 && first_end < text.find('\n').unwrap(),
            "{first_end}"
        );
        assert_eq!(edge(2, false), 0);
        // Pressed again, it stays; back from there is the same line's start.
        assert_eq!(edge(first_end, true), first_end);
        assert_eq!(edge(first_end, false), 0);
        // After the line break.
        let last = text.len() - 1;
        assert_eq!(edge(last, true), text.len());
        assert_eq!(edge(last, false), text.find('\n').unwrap() + 1);
    }
}
