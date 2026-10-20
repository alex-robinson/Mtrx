//! A typeset message (an image) whose text can be selected: the renderer
//! says where each piece of text is drawn ([`mtrx_format::Piece`]), mapped
//! here onto the message's units ([`crate::message::TextMap`]). Formulas and
//! code are pieces selected whole, and copy as their source.

use std::{ops::Range, rc::Rc, sync::Arc};

use iced::{
    Element, Event, Length, Point, Rectangle, Renderer, Size, Theme,
    advanced::{
        Clipboard, Layout, Shell, Widget,
        layout::{Limits, Node},
        mouse, renderer,
        widget::{Tree, tree},
    },
    keyboard,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    message::TextMap,
    selection::{Gesture, Pos, unit_range},
    style,
};

/// A piece of text in the image: its bytes in a unit, and where it is drawn
/// in logical pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub unit: usize,
    pub range: Range<usize>,
    /// A formula or code, only selected whole.
    pub whole: bool,
    pub rect: Rectangle,
}

/// What the image shows, for selecting: its pieces in reading order, and
/// the text of each unit.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Text {
    pub pieces: Vec<Piece>,
    pub units: Vec<String>,
}

impl Text {
    /// `pieces` from the renderer (in pixels, `scale` per logical pixel)
    /// placed in the message's units.
    pub fn new(pieces: &[mtrx_format::Piece], map: &TextMap, scale: f32) -> Self {
        let pieces = pieces
            .iter()
            .filter_map(|piece| {
                let leaf = map.leaves.get(piece.leaf)?;
                let range = match &piece.range {
                    Some(range) if range.end <= leaf.len => {
                        leaf.start + range.start..leaf.start + range.end
                    }
                    Some(_) => return None,
                    None => leaf.start..leaf.start + leaf.len,
                };
                let [x, y, width, height] = piece.rect.map(|v| v / scale);
                Some(Piece {
                    unit: leaf.unit,
                    range,
                    whole: piece.range.is_none(),
                    rect: Rectangle::new(Point::new(x, y), Size::new(width, height)),
                })
            })
            .collect();
        let units = map.units.iter().map(|u| u.text.clone()).collect();
        Self { pieces, units }
    }

    /// The place nearest to `point`: on the line under it (else the nearest
    /// line), before or after the piece there.
    fn hit(&self, point: Point) -> Option<Pos> {
        let nearest = self.pieces.iter().min_by(|a, b| {
            vertical_distance(&a.rect, point.y).total_cmp(&vertical_distance(&b.rect, point.y))
        })?;
        let line: Vec<&Piece> = self
            .pieces
            .iter()
            .filter(|p| overlap(&p.rect, &nearest.rect))
            .collect();
        let piece = line
            .iter()
            .find(|p| point.x >= p.rect.x && point.x < p.rect.x + p.rect.width)
            .or_else(|| {
                line.iter().min_by(|a, b| {
                    horizontal_distance(&a.rect, point.x)
                        .total_cmp(&horizontal_distance(&b.rect, point.x))
                })
            })?;
        let before = point.x < piece.rect.center_x();
        Some(Pos {
            unit: piece.unit,
            offset: if before {
                piece.range.start
            } else {
                piece.range.end
            },
        })
    }

    /// The word at `pos`, or the formula there.
    fn word_at(&self, pos: Pos) -> Range<usize> {
        if let Some(piece) = self.pieces.iter().find(|p| {
            p.whole && p.unit == pos.unit && (p.range.start..=p.range.end).contains(&pos.offset)
        }) {
            return piece.range.clone();
        }
        let text = self.units.get(pos.unit).map_or("", String::as_str);
        text.split_word_bound_indices()
            .map(|(start, word)| start..start + word.len())
            .find(|word| word.contains(&pos.offset) || word.end == pos.offset)
            .unwrap_or(pos.offset..pos.offset)
    }

    /// Rectangles of the pieces in the selected part.
    fn highlight(&self, selected: (Pos, Pos)) -> impl Iterator<Item = Rectangle> + '_ {
        self.pieces.iter().filter_map(move |piece| {
            let len = self.units.get(piece.unit)?.len();
            let range = unit_range(selected, piece.unit, len)?;
            (range.start < piece.range.end && piece.range.start < range.end).then_some(piece.rect)
        })
    }
}

fn vertical_distance(rect: &Rectangle, y: f32) -> f32 {
    if y < rect.y {
        rect.y - y
    } else if y > rect.y + rect.height {
        y - rect.y - rect.height
    } else {
        0.0
    }
}

fn horizontal_distance(rect: &Rectangle, x: f32) -> f32 {
    if x < rect.x {
        rect.x - x
    } else if x > rect.x + rect.width {
        x - rect.x - rect.width
    } else {
        0.0
    }
}

fn overlap(a: &Rectangle, b: &Rectangle) -> bool {
    a.y < b.y + b.height && b.y < a.y + a.height
}

pub struct SelectableImage<'a, Message> {
    image: Element<'a, Message>,
    text: Arc<Text>,
    selected: Option<(Pos, Pos)>,
    dragging: bool,
    on_gesture: Option<Rc<dyn Fn(Gesture) -> Message + 'a>>,
}

/// `image` showing `text`.
pub fn selectable_image<'a, Message>(
    image: impl Into<Element<'a, Message>>,
    text: Arc<Text>,
) -> SelectableImage<'a, Message> {
    SelectableImage {
        image: image.into(),
        text,
        selected: None,
        dragging: false,
        on_gesture: None,
    }
}

impl<'a, Message> SelectableImage<'a, Message> {
    /// The part of the message that is selected.
    pub fn selected(mut self, selected: Option<(Pos, Pos)>) -> Self {
        self.selected = selected;
        self
    }

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
    last_click: Option<mouse::Click>,
    modifiers: keyboard::Modifiers,
}

impl<Message> Widget<Message, Theme, Renderer> for SelectableImage<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.image)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[self.image.as_widget()]);
    }

    fn size(&self) -> Size<Length> {
        self.image.as_widget().size()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &Limits) -> Node {
        self.image
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;
        let bounds = layout.bounds();
        if let Some(selected) = self.selected {
            let translation = bounds.position() - Point::ORIGIN;
            for rect in self.text.highlight(selected) {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: rect + translation,
                        ..Default::default()
                    },
                    style::selection(theme),
                );
            }
        }
        self.image.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
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
        let Some(on_gesture) = &self.on_gesture else {
            return;
        };
        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();
        let origin = bounds.position() - Point::ORIGIN;
        match event {
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.modifiers = *modifiers;
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(at) = cursor.position_in(bounds) else {
                    return;
                };
                let Some(pos) = self.text.hit(at) else {
                    return;
                };
                let position = cursor.position().unwrap_or_default();
                let click = mouse::Click::new(position, mouse::Button::Left, state.last_click);
                state.last_click = Some(click);
                let (unit, from, to) = match click.kind() {
                    mouse::click::Kind::Single => (pos.unit, pos.offset, pos.offset),
                    mouse::click::Kind::Double => {
                        let word = self.text.word_at(pos);
                        (pos.unit, word.start, word.end)
                    }
                    mouse::click::Kind::Triple => {
                        let len = self.text.units.get(pos.unit).map_or(0, String::len);
                        (pos.unit, 0, len)
                    }
                };
                shell.publish(on_gesture(Gesture::Press {
                    unit,
                    from,
                    to,
                    extend: state.modifiers.shift(),
                }));
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if self.dragging => {
                // The cursor: inside a scrollable, it is in the content's
                // coordinates.
                let Some(position) = cursor.land().position() else {
                    return;
                };
                let beside = position.y >= bounds.y && position.y < bounds.y + bounds.height;
                if beside && let Some(pos) = self.text.hit(position - origin) {
                    shell.publish(on_gesture(Gesture::Drag {
                        unit: pos.unit,
                        offset: pos.offset,
                    }));
                }
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if self.on_gesture.is_some() && (self.dragging || cursor.is_over(layout.bounds())) {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::None
        }
    }
}

impl<'a, Message: 'a> From<SelectableImage<'a, Message>> for Element<'a, Message> {
    fn from(widget: SelectableImage<'a, Message>) -> Self {
        Element::new(widget)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn piece(unit: usize, range: Range<usize>, whole: bool, x: f32, y: f32, w: f32) -> Piece {
        Piece {
            unit,
            range,
            whole,
            rect: Rectangle::new(Point::new(x, y), Size::new(w, 20.0)),
        }
    }

    /// "Rate $m$ ok" on one line, "Next" below.
    fn text() -> Text {
        Text {
            pieces: vec![
                piece(0, 0..1, false, 0.0, 0.0, 10.0),
                piece(0, 1..2, false, 10.0, 0.0, 10.0),
                piece(0, 2..3, false, 20.0, 0.0, 10.0),
                piece(0, 3..4, false, 30.0, 0.0, 10.0),
                piece(0, 4..5, false, 40.0, 0.0, 5.0),
                piece(0, 5..8, true, 45.0, -4.0, 30.0),
                piece(0, 8..9, false, 75.0, 0.0, 5.0),
                piece(0, 9..10, false, 80.0, 0.0, 10.0),
                piece(0, 10..11, false, 90.0, 0.0, 10.0),
                piece(1, 0..4, false, 0.0, 30.0, 40.0),
            ],
            units: vec!["Rate $m$ ok".into(), "Next".into()],
        }
    }

    fn pos(unit: usize, offset: usize) -> Pos {
        Pos { unit, offset }
    }

    #[test]
    fn hits_glyphs_lines_and_whole_formulas() {
        let text = text();
        // Left and right half of a glyph.
        assert_eq!(text.hit(Point::new(12.0, 10.0)), Some(pos(0, 1)));
        assert_eq!(text.hit(Point::new(18.0, 10.0)), Some(pos(0, 2)));
        // A formula is before or after, never inside.
        assert_eq!(text.hit(Point::new(50.0, 10.0)), Some(pos(0, 5)));
        assert_eq!(text.hit(Point::new(70.0, 10.0)), Some(pos(0, 8)));
        // Beside the line, and below everything.
        assert_eq!(text.hit(Point::new(500.0, 10.0)), Some(pos(0, 11)));
        assert_eq!(text.hit(Point::new(-5.0, 35.0)), Some(pos(1, 0)));
        assert_eq!(text.hit(Point::new(500.0, 400.0)), Some(pos(1, 4)));
        // Words and formulas for double clicks.
        assert_eq!(text.word_at(pos(0, 2)), 0..4);
        assert_eq!(text.word_at(pos(0, 5)), 5..8);
    }

    #[test]
    fn highlights_the_selected_pieces() {
        let text = text();
        let lit: Vec<f32> = text
            .highlight((pos(0, 3), pos(1, 2)))
            .map(|rect| rect.x)
            .collect();
        assert_eq!(lit, [30.0, 40.0, 45.0, 75.0, 80.0, 90.0, 0.0]);
    }
}
