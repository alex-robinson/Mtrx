//! Rich text whose text can be selected, like iced's `rich_text` (whose
//! layout and drawing it follows) plus hit-testing and highlighting through
//! the paragraph's cosmic-text layout. The selection itself lives in the
//! room ([`crate::selection`]); this widget reports gestures and draws the
//! part it is given.

use std::{ops::Range, rc::Rc};

use iced::{
    Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Renderer, Size, Theme, Vector,
    advanced::{
        Clipboard, Layout, Shell, Widget, layout, mouse, renderer,
        text::{self as core_text, Paragraph as _, Span},
        widget::{
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

use crate::{selection::Gesture, style};

/// How far the cursor may move between pressing and releasing a link for it
/// to count as a click rather than the start of a selection.
const CLICK_SLOP: f32 = 4.0;

pub struct Selectable<'a, Message> {
    spans: Vec<Span<'static, String, Font>>,
    /// Its index among the message's units.
    unit: usize,
    size: Option<Pixels>,
    /// The selected part of its text, in bytes.
    selected: Option<Range<usize>>,
    dragging: bool,
    on_link: Option<fn(String) -> Message>,
    on_gesture: Option<Rc<dyn Fn(Gesture) -> Message + 'a>>,
}

/// Unit `unit` of a message, showing `spans`.
pub fn selectable<'a, Message>(
    spans: Vec<Span<'static, String, Font>>,
    unit: usize,
) -> Selectable<'a, Message> {
    Selectable {
        spans,
        unit,
        size: None,
        selected: None,
        dragging: false,
        on_link: None,
        on_gesture: None,
    }
}

impl<'a, Message> Selectable<'a, Message> {
    pub fn size(mut self, size: Pixels) -> Self {
        self.size = Some(size);
        self
    }

    /// Sent when a link is clicked (pressed and released without moving).
    pub fn on_link(mut self, on_link: fn(String) -> Message) -> Self {
        self.on_link = Some(on_link);
        self
    }

    pub fn selected(mut self, range: Option<Range<usize>>) -> Self {
        self.selected = range;
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
    paragraph: Paragraph,
    /// The spans' text, and where each of its lines starts.
    text: String,
    line_starts: Vec<usize>,
    last_click: Option<mouse::Click>,
    modifiers: keyboard::Modifiers,
    /// The link span pressed, and where.
    link_pressed: Option<(usize, Point)>,
    hovered_link: Option<usize>,
}

impl State {
    fn set_spans(&mut self, spans: &[Span<'static, String, Font>]) {
        self.spans = spans.to_vec();
        self.text = spans.iter().map(|s| s.text.as_ref()).collect();
        self.line_starts = std::iter::once(0)
            .chain(self.text.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
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

    /// The word (or run of spaces or punctuation) around `offset`.
    fn word_at(&self, offset: usize) -> Range<usize> {
        self.text
            .split_word_bound_indices()
            .map(|(start, word)| start..start + word.len())
            .find(|word| word.contains(&offset))
            .unwrap_or(offset..offset)
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
                wrapping: Wrapping::default(),
            };
            if state.spans != self.spans {
                state.paragraph = Paragraph::with_spans(text(self.spans.as_slice()));
                state.set_spans(&self.spans);
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

        if let Some(range) = &self.selected {
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
            core_widget_text::Style::default(),
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
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();
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
                let click = mouse::Click::new(position, mouse::Button::Left, state.last_click);
                state.last_click = Some(click);
                let offset = state.offset_at(at, bounds.size());
                let (from, to) = match click.kind() {
                    mouse::click::Kind::Single => (offset, offset),
                    mouse::click::Kind::Double => {
                        let word = state.word_at(offset);
                        (word.start, word.end)
                    }
                    mouse::click::Kind::Triple => (0, state.text.len()),
                };
                shell.publish(on_gesture(Gesture::Press {
                    unit: self.unit,
                    from,
                    to,
                    extend: state.modifiers.shift(),
                }));
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if let Some((_, pressed)) = state.link_pressed
                    && pressed.distance(*position) > CLICK_SLOP
                {
                    state.link_pressed = None;
                }
                // Over its lines, also left or right of the text.
                let beside = position.y >= bounds.y && position.y < bounds.y + bounds.height;
                if self.dragging
                    && beside
                    && let Some(on_gesture) = &self.on_gesture
                {
                    let offset = state.offset_at(*position - translation(bounds), bounds.size());
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
        } else if self.on_gesture.is_some() && (self.dragging || cursor.is_over(layout.bounds())) {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::None
        }
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
