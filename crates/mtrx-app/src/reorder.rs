//! Dragging a list's items by their grips to other places, as for a room's
//! calls and links. The list's area ends the drag: releasing the button over
//! it drops the item where the line shows, leaving it gives up.

use iced::{
    Color, Element, Length, Theme, mouse,
    widget::{column, container, mouse_area, row, space},
};

use crate::{icons, style};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    /// The grip of the item at this place was pressed.
    Grab(usize),
    /// While dragging, the cursor entered the item at this place.
    Over(usize),
    /// The button was released over the list.
    Drop,
    /// The cursor left the list.
    Leave,
}

/// The item being dragged and the one under the cursor.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Reorder {
    drag: Option<(usize, usize)>,
}

impl Reorder {
    /// The move a drop makes: the item at the first place goes to the
    /// second.
    pub fn update(&mut self, message: Message) -> Option<(usize, usize)> {
        match message {
            Message::Grab(at) => self.drag = Some((at, at)),
            Message::Over(at) => {
                if let Some((_, over)) = &mut self.drag {
                    *over = at;
                }
            }
            Message::Drop => {
                let (from, to) = self.drag.take()?;
                return (from != to).then_some((from, to));
            }
            Message::Leave => self.drag = None,
        }
        None
    }

    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// The gap the dragged item would land in, counted from 0 before the
    /// first item; none over its own place.
    fn landing(&self) -> Option<usize> {
        let (from, over) = self.drag?;
        match over.cmp(&from) {
            std::cmp::Ordering::Less => Some(over),
            std::cmp::Ordering::Equal => None,
            std::cmp::Ordering::Greater => Some(over + 1),
        }
    }
}

/// Move the item at `from` to place `to` among `items`.
pub fn apply<T>(items: &mut [T], (from, to): (usize, usize)) {
    if from < to {
        items[from..=to].rotate_left(1);
    } else if to < from {
        items[to..=from].rotate_right(1);
    }
}

/// `items`, each after a grip while `movable`, with a line where the
/// dragged one would land.
pub fn list<'a, M: Clone + 'a>(
    reorder: &Reorder,
    items: Vec<Element<'a, M>>,
    movable: bool,
    message: impl Fn(Message) -> M,
) -> Element<'a, M> {
    if !movable {
        return column(items).into();
    }
    let landing = reorder.landing();
    let count = items.len();
    let mut list = column![];
    for (at, item) in items.into_iter().enumerate() {
        let grip = mouse_area(icons::grip(14.0, grip_color))
            .on_press(message(Message::Grab(at)))
            .interaction(mouse::Interaction::Grab);
        let item = mouse_area(
            row![grip, container(item).width(Length::Fill)]
                .spacing(2)
                .align_y(iced::Alignment::Center),
        )
        .on_enter(message(Message::Over(at)));
        list = list.push(line(landing == Some(at))).push(item);
    }
    list = list.push(line(landing == Some(count)));
    let area = mouse_area(list)
        .on_release(message(Message::Drop))
        .on_exit(message(Message::Leave));
    if reorder.dragging() {
        area.interaction(mouse::Interaction::Grabbing).into()
    } else {
        area.into()
    }
}

fn grip_color(theme: &Theme) -> Color {
    theme
        .extended_palette()
        .background
        .base
        .text
        .scale_alpha(0.45)
}

/// The gap between two items, a line where a dragged one would land.
fn line<'a, M: 'a>(active: bool) -> Element<'a, M> {
    container(space())
        .width(Length::Fill)
        .height(2)
        .style(move |theme| style::drop_line(theme, active))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drags_items_to_other_places() {
        let mut reorder = Reorder::default();
        let mut items = ['a', 'b', 'c'];
        assert_eq!(reorder.update(Message::Over(2)), None, "not dragging");
        reorder.update(Message::Grab(0));
        reorder.update(Message::Over(2));
        assert_eq!(reorder.landing(), Some(3), "after the last");
        let moved = reorder.update(Message::Drop).unwrap();
        apply(&mut items, moved);
        assert_eq!(items, ['b', 'c', 'a']);

        reorder.update(Message::Grab(2));
        reorder.update(Message::Over(1));
        assert_eq!(reorder.landing(), Some(1));
        apply(&mut items, reorder.update(Message::Drop).unwrap());
        assert_eq!(items, ['b', 'a', 'c']);

        reorder.update(Message::Grab(1));
        assert_eq!(reorder.landing(), None, "over its own place");
        assert_eq!(reorder.update(Message::Drop), None);
        reorder.update(Message::Grab(1));
        reorder.update(Message::Leave);
        assert_eq!(reorder.update(Message::Drop), None, "left the list");
        assert!(!reorder.dragging());
    }
}
