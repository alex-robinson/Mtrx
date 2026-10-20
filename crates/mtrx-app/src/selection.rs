//! Selecting text in the timeline, across messages: where a selection
//! starts and ends, which part of each message it covers, and the text it
//! copies. Messages are split into units (paragraphs, headings, code
//! blocks), each drawn by a [`crate::selectable`] widget. See
//! `docs/text-selection.md`.

use std::collections::HashMap;

/// A place in a message: a unit and a byte offset into its text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pos {
    pub unit: usize,
    pub offset: usize,
}

impl Pos {
    /// After everything in a message.
    pub const END: Self = Self {
        unit: usize::MAX,
        offset: usize::MAX,
    };
}

/// A place in the timeline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Point {
    /// [`mtrx_core::Event::id`] of the message.
    pub event: String,
    pub pos: Pos,
}

/// What a selectable unit reports.
#[derive(Clone, Debug)]
pub enum Gesture {
    /// Pressed: a new selection from `from` to `to` (a word or the whole
    /// unit when double or triple clicked), or with `extend` (Shift) the
    /// selection extended to `to`.
    Press {
        unit: usize,
        from: usize,
        to: usize,
        extend: bool,
    },
    /// While dragging, the cursor is over this unit at `offset`.
    Drag { unit: usize, offset: usize },
}

#[derive(Clone, Debug)]
pub enum Message {
    /// A gesture in a message.
    Gesture(String, Gesture),
    /// The mouse button was released.
    End,
    /// ⌘C: put the selected text on the clipboard.
    Copy,
    /// ⌘A: select the messages on screen, from the first to the last.
    SelectAll,
    All(String, String),
    Clear,
}

/// Text selected from `anchor` (where it started) to `focus` (where it
/// ends), in either order.
#[derive(Clone, Debug)]
pub struct Selection {
    pub anchor: Point,
    pub focus: Point,
    /// The mouse button is still down, so moving extends it.
    pub dragging: bool,
}

impl Selection {
    pub fn update(selection: &mut Option<Self>, message: Message) {
        match message {
            Message::Gesture(
                event,
                Gesture::Press {
                    unit,
                    from,
                    to,
                    extend,
                },
            ) => {
                let focus = Point {
                    event: event.clone(),
                    pos: Pos { unit, offset: to },
                };
                match selection {
                    Some(selection) if extend => {
                        selection.focus = focus;
                        selection.dragging = true;
                    }
                    _ => {
                        *selection = Some(Self {
                            anchor: Point {
                                event,
                                pos: Pos { unit, offset: from },
                            },
                            focus,
                            dragging: true,
                        });
                    }
                }
            }
            Message::Gesture(event, Gesture::Drag { unit, offset }) => {
                if let Some(selection) = selection.as_mut().filter(|s| s.dragging) {
                    selection.focus = Point {
                        event,
                        pos: Pos { unit, offset },
                    };
                }
            }
            Message::Clear => *selection = None,
            Message::All(first, last) => {
                *selection = Some(Self {
                    anchor: Point {
                        event: first,
                        pos: Pos::default(),
                    },
                    focus: Point {
                        event: last,
                        pos: Pos::END,
                    },
                    dragging: false,
                });
            }
            Message::End => {
                if let Some(selection) = selection {
                    selection.dragging = false;
                }
            }
            // Handled by the room, which has the text and the view.
            Message::Copy | Message::SelectAll => {}
        }
    }

    /// Start and end in timeline order, given each message's index.
    pub fn ordered(&self, order: &HashMap<&str, usize>) -> Option<Bounds> {
        let at = |point: &Point| Some((*order.get(point.event.as_str())?, point.pos));
        let (a, f) = (at(&self.anchor)?, at(&self.focus)?);
        let (start, end) = if a <= f { (a, f) } else { (f, a) };
        (start != end).then_some(Bounds { start, end })
    }
}

/// A non-empty selection as (message index, place) pairs, start first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    pub start: (usize, Pos),
    pub end: (usize, Pos),
}

impl Bounds {
    /// The part of the message at `index` it covers, if any.
    pub fn in_message(&self, index: usize) -> Option<(Pos, Pos)> {
        if index < self.start.0 || index > self.end.0 {
            return None;
        }
        let from = if index == self.start.0 {
            self.start.1
        } else {
            Pos::default()
        };
        let to = if index == self.end.0 {
            self.end.1
        } else {
            Pos::END
        };
        Some((from, to))
    }
}

/// The byte range of unit `unit` (of `len` bytes) between `from` and `to`,
/// if not empty.
pub fn unit_range(
    (from, to): (Pos, Pos),
    unit: usize,
    len: usize,
) -> Option<std::ops::Range<usize>> {
    if unit < from.unit || unit > to.unit {
        return None;
    }
    let start = if unit == from.unit {
        from.offset.min(len)
    } else {
        0
    };
    let end = if unit == to.unit {
        to.offset.min(len)
    } else {
        len
    };
    (start < end).then_some(start..end)
}

/// A selectable piece of a message, as shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unit {
    pub text: String,
    /// Put before its text when copied from its start along with other
    /// units, e.g. "> " in a quote or "1. " for a list item.
    pub prefix: String,
    /// Put between the previous unit and this one.
    pub gap: &'static str,
}

/// The text of `units` between `from` and `to`.
pub fn units_text(units: &[Unit], from: Pos, to: Pos) -> String {
    let mut out = String::new();
    let several = from.unit != to.unit;
    let mut first = true;
    for (i, unit) in units.iter().enumerate() {
        let Some(range) = unit_range((from, to), i, unit.text.len()) else {
            continue;
        };
        if !first {
            out.push_str(unit.gap);
        }
        if several && range.start == 0 {
            out.push_str(&unit.prefix);
        }
        out.push_str(&unit.text[range]);
        first = false;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(text: &str, prefix: &str, gap: &'static str) -> Unit {
        Unit {
            text: text.into(),
            prefix: prefix.into(),
            gap,
        }
    }

    fn pos(unit: usize, offset: usize) -> Pos {
        Pos { unit, offset }
    }

    #[test]
    fn copies_the_selected_parts_with_structure() {
        let units = [
            unit("Melt rates", "", ""),
            unit("first", "- ", "\n\n"),
            unit("second", "- ", "\n"),
        ];
        assert_eq!(units_text(&units, pos(0, 5), pos(0, 10)), "rates");
        assert_eq!(
            units_text(&units, pos(0, 5), pos(2, 3)),
            "rates\n\n- first\n- sec"
        );
        assert_eq!(
            units_text(&units, Pos::default(), Pos::END),
            "Melt rates\n\n- first\n- second"
        );
        // Inside one unit, no prefix.
        assert_eq!(units_text(&units, pos(1, 0), pos(1, 5)), "first");
    }

    #[test]
    fn orders_points_and_splits_by_message() {
        let point = |event: &str, unit, offset| Point {
            event: event.into(),
            pos: pos(unit, offset),
        };
        let order = HashMap::from([("$a", 3), ("$b", 4), ("$c", 7)]);
        // Dragged upwards: the focus comes first.
        let selection = Selection {
            anchor: point("$c", 0, 4),
            focus: point("$a", 1, 2),
            dragging: false,
        };
        let bounds = selection.ordered(&order).unwrap();
        assert_eq!(bounds.start, (3, pos(1, 2)));
        assert_eq!(bounds.in_message(3), Some((pos(1, 2), Pos::END)));
        assert_eq!(bounds.in_message(4), Some((Pos::default(), Pos::END)));
        assert_eq!(bounds.in_message(7), Some((Pos::default(), pos(0, 4))));
        assert_eq!(bounds.in_message(8), None);
        assert_eq!(unit_range((pos(1, 2), Pos::END), 1, 10), Some(2..10));
        assert_eq!(unit_range((pos(1, 2), Pos::END), 0, 10), None);

        // A click selects nothing.
        let click = Selection {
            anchor: point("$a", 0, 1),
            focus: point("$a", 0, 1),
            dragging: true,
        };
        assert_eq!(click.ordered(&order), None);
    }
}
