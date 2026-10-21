//! Text of at most a few lines (one by default) that ends in "…" when it
//! does not fit (iced has no built-in ellipsis). The fitted paragraph is cached until the text or the
//! available width changes.

use iced::{
    Color, Element, Font, Length, Pixels, Rectangle, Size, Theme,
    advanced::{
        Layout, Widget, layout, mouse, renderer,
        text::{self as core_text, Paragraph as _},
        widget::{Tree, tree},
    },
    alignment,
    widget::text,
};
use std::borrow::Cow;

use unicode_segmentation::UnicodeSegmentation;

const ELLIPSIS: &str = "…";

pub struct Ellipsis<'a> {
    content: Cow<'a, str>,
    size: Pixels,
    font: Font,
    width: Length,
    lines: usize,
    style: fn(&Theme) -> text::Style,
}

pub fn ellipsis<'a>(content: impl Into<Cow<'a, str>>) -> Ellipsis<'a> {
    Ellipsis {
        content: content.into(),
        size: Pixels(crate::style::TEXT_SIZE),
        font: crate::fonts::INTER,
        width: Length::Shrink,
        lines: 1,
        style: text::default,
    }
}

impl Ellipsis<'_> {
    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.size = size.into();
        self
    }

    pub fn font(mut self, font: Font) -> Self {
        self.font = font;
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Wrap onto up to `lines` lines.
    pub fn lines(mut self, lines: usize) -> Self {
        self.lines = lines.max(1);
        self
    }

    pub fn style(mut self, style: fn(&Theme) -> text::Style) -> Self {
        self.style = style;
        self
    }

    fn paragraph<P: core_text::Paragraph<Font = Font>>(&self, content: &str, max_width: f32) -> P {
        let wraps = self.lines > 1;
        P::with_text(core_text::Text {
            content,
            bounds: if wraps {
                Size::new(max_width, f32::INFINITY)
            } else {
                Size::INFINITE
            },
            size: self.size,
            line_height: core_text::LineHeight::default(),
            font: self.font,
            align_x: core_text::Alignment::Default,
            align_y: alignment::Vertical::Top,
            shaping: core_text::Shaping::Advanced,
            wrapping: if wraps {
                core_text::Wrapping::WordOrGlyph
            } else {
                core_text::Wrapping::None
            },
        })
    }

    fn fits<P: core_text::Paragraph<Font = Font>>(&self, paragraph: &P, max_width: f32) -> bool {
        if self.lines == 1 {
            paragraph.min_width() <= max_width
        } else {
            let line = core_text::LineHeight::default().to_absolute(self.size).0;
            paragraph.min_height() <= self.lines as f32 * line + 0.5
        }
    }

    /// The longest prefix (whole graphemes) plus "…" within `max_width`
    /// and the lines allowed.
    fn fit<P: core_text::Paragraph<Font = Font>>(&self, max_width: f32) -> P {
        let full: P = self.paragraph(&self.content, max_width);
        if self.fits(&full, max_width) {
            return full;
        }
        let cuts: Vec<usize> = self
            .content
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .collect();
        let shortened = |n: usize| -> P {
            let prefix = self.content[..cuts[n]].trim_end();
            self.paragraph(&format!("{prefix}{ELLIPSIS}"), max_width)
        };
        // The largest grapheme count whose shortened text fits; cutting
        // after zero graphemes (just "…") is the floor.
        let (mut low, mut high) = (0, cuts.len() - 1);
        while low < high {
            let middle = (low + high).div_ceil(2);
            if self.fits(&shortened(middle), max_width) {
                low = middle;
            } else {
                high = middle - 1;
            }
        }
        shortened(low)
    }
}

struct State<P> {
    /// Text, width, size, font and lines the paragraph was fitted for.
    fitted: Option<(String, f32, Pixels, Font, usize)>,
    paragraph: P,
}

impl<Message, Renderer> Widget<Message, Theme, Renderer> for Ellipsis<'_>
where
    Renderer: core_text::Renderer<Font = Font>,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State<Renderer::Paragraph>>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::<Renderer::Paragraph> {
            fitted: None,
            paragraph: Default::default(),
        })
    }

    fn size(&self) -> Size<Length> {
        Size::new(self.width, Length::Shrink)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_mut::<State<Renderer::Paragraph>>();
        let max_width = limits.max().width;
        let current = state
            .fitted
            .as_ref()
            .is_some_and(|(content, width, size, font, lines)| {
                *content == self.content
                    && *width == max_width
                    && *size == self.size
                    && *font == self.font
                    && *lines == self.lines
            });
        if !current {
            state.paragraph = self.fit(max_width);
            state.fitted = Some((
                self.content.to_string(),
                max_width,
                self.size,
                self.font,
                self.lines,
            ));
        }
        let bounds = state.paragraph.min_bounds();
        layout::Node::new(limits.resolve(self.width, Length::Shrink, bounds))
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State<Renderer::Paragraph>>();
        let color: Color = (self.style)(theme).color.unwrap_or(style.text_color);
        let bounds = layout.bounds();
        renderer.fill_paragraph(&state.paragraph, bounds.position(), color, bounds);
    }
}

impl<'a, Message, Renderer> From<Ellipsis<'a>> for Element<'a, Message, Theme, Renderer>
where
    Renderer: core_text::Renderer<Font = Font> + 'a,
{
    fn from(ellipsis: Ellipsis<'a>) -> Self {
        Element::new(ellipsis)
    }
}

/// `text` followed by `after` (a count, say), with `text` shortened to
/// leave room for it: `after` stays right next to the text however long
/// it is. Fills the width given.
pub fn followed_by<'a, Message: 'a>(
    text: Ellipsis<'a>,
    after: impl Into<Element<'a, Message>>,
    spacing: f32,
) -> Element<'a, Message> {
    Element::new(FollowedBy {
        text: text.into(),
        after: after.into(),
        spacing,
    })
}

struct FollowedBy<'a, Message> {
    text: Element<'a, Message>,
    after: Element<'a, Message>,
    spacing: f32,
}

impl<Message> Widget<Message, Theme, iced::Renderer> for FollowedBy<'_, Message> {
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.text), Tree::new(&self.after)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[self.text.as_widget(), self.after.as_widget()]);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let max = limits.max();
        let after = self.after.as_widget_mut().layout(
            &mut tree.children[1],
            renderer,
            &layout::Limits::new(Size::ZERO, max),
        );
        let room = (max.width - after.size().width - self.spacing).max(0.0);
        let text = self.text.as_widget_mut().layout(
            &mut tree.children[0],
            renderer,
            &layout::Limits::new(Size::ZERO, Size::new(room, max.height)),
        );
        let height = text.size().height.max(after.size().height);
        let text_size = text.size();
        let after_size = after.size();
        let text = text.move_to((0.0, (height - text_size.height) / 2.0));
        let after = after.move_to((
            text_size.width + self.spacing,
            (height - after_size.height) / 2.0,
        ));
        layout::Node::with_children(
            limits.resolve(Length::Fill, Length::Shrink, Size::new(max.width, height)),
            vec![text, after],
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        for ((child, state), layout) in [&self.text, &self.after]
            .into_iter()
            .zip(&tree.children)
            .zip(layout.children())
        {
            child
                .as_widget()
                .draw(state, renderer, theme, style, layout, cursor, viewport);
        }
    }
}
