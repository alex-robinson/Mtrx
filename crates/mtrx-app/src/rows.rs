//! The timeline as it is shown, row by row: messages under their sender's
//! name, runs of state events in one line, days between. Each row has parts
//! whose text can be selected, in reading order. The view and the selection
//! both go through here, so the text copied is the text shown. See
//! `docs/text-selection.md`.

use std::{
    collections::HashMap,
    time::{Duration, SystemTime},
};

use chrono::{DateTime, Local};
use iced::advanced::widget::Id;
use mtrx_core::{Event, EventContent, MessageKind, Reply, TimelineEntry};
use mtrx_format::Document;

use crate::{
    message, room,
    selection::{Bounds, Shown, Unit, units_text},
};

/// A message this soon after the one before, by the same sender, goes
/// under the same name.
const GROUP_GAP: Duration = Duration::from_secs(5 * 60);

/// A row of the timeline.
pub enum Row<'a> {
    /// An event; with `header`, its sender's name and time above it.
    Event {
        event: &'a Event,
        header: bool,
    },
    /// Consecutive state events (joins, settings), in one line.
    State(Vec<&'a Event>),
    Day(SystemTime),
    Start,
}

/// The rows showing `entries`.
pub fn rows(entries: &[TimelineEntry]) -> Vec<Row<'_>> {
    let mut rows = Vec::new();
    let mut previous: Option<&Event> = None;
    let mut state: Vec<&Event> = Vec::new();
    for entry in entries {
        if let TimelineEntry::Event(event) = entry
            && event.is_state()
        {
            state.push(event);
            continue;
        }
        if !state.is_empty() {
            rows.push(Row::State(std::mem::take(&mut state)));
            previous = None;
        }
        match entry {
            TimelineEntry::Event(event) => {
                let grouped = previous.is_some_and(|p| {
                    p.sender == event.sender
                        && event
                            .timestamp
                            .duration_since(p.timestamp)
                            .unwrap_or_default()
                            < GROUP_GAP
                });
                rows.push(Row::Event {
                    event,
                    header: !grouped,
                });
                previous = matches!(event.content, EventContent::Message(_)).then_some(event);
            }
            TimelineEntry::DayDivider(day) => {
                rows.push(Row::Day(*day));
                previous = None;
            }
            TimelineEntry::Start => rows.push(Row::Start),
            TimelineEntry::ReadMarker => {}
        }
    }
    if !state.is_empty() {
        rows.push(Row::State(state));
    }
    rows
}

/// A part of a row whose text can be selected.
pub struct Part<'a> {
    /// Its place in a selection ([`crate::selection::Point::event`]): a
    /// message's text has the message's event id.
    pub key: String,
    pub text: Text<'a>,
}

/// What a part shows.
pub enum Text<'a> {
    /// The sender's name and the time, above their messages.
    Sender(&'a Event),
    /// The message replied to: its sender's name and the start of it.
    Reply(&'a Reply),
    /// A message's text.
    Message(&'a Document),
    /// A line: in place of a message (deleted, say), for state events, a
    /// day, the start of the room.
    Line(String),
}

impl<'a> Row<'a> {
    /// Its parts, in reading order.
    pub fn parts(&self) -> Vec<Part<'a>> {
        let line = |key: String, line: String| Part {
            key,
            text: Text::Line(line),
        };
        match self {
            Row::Event { event, header } => {
                let mut parts = Vec::new();
                if *header {
                    parts.push(Part {
                        key: format!("{}/sender", event.id),
                        text: Text::Sender(event),
                    });
                }
                match &event.content {
                    EventContent::Message(m) => {
                        if let Some(reply) = &m.in_reply_to {
                            parts.push(Part {
                                key: reply_key(&event.id),
                                text: Text::Reply(reply),
                            });
                        }
                        if matches!(
                            m.kind,
                            MessageKind::Text | MessageKind::Notice | MessageKind::Emote
                        ) {
                            parts.push(Part {
                                key: event.id.clone(),
                                text: Text::Message(&m.document),
                            });
                        }
                    }
                    content => {
                        parts.extend(placeholder(content).map(|l| line(event.id.clone(), l)))
                    }
                }
                parts
            }
            Row::State(events) => vec![line(
                format!("{}/state", events[0].id),
                room::state_summary(events),
            )],
            Row::Day(day) => {
                let since = day
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default();
                let day: DateTime<Local> = (*day).into();
                vec![line(
                    format!("day/{}", since.as_secs()),
                    day.format("%A, %e %B %Y").to_string(),
                )]
            }
            Row::Start => vec![line(
                "start".into(),
                "This is the start of the room.".into(),
            )],
        }
    }

    /// The event it shows, if it does.
    fn event(&self) -> Option<&'a Event> {
        match self {
            Row::Event { event, .. } => Some(event),
            _ => None,
        }
    }
}

/// The line shown in place of a message that is not one, if it is shown.
fn placeholder(content: &EventContent) -> Option<String> {
    match content {
        EventContent::UnableToDecrypt => {
            Some("Unable to decrypt this message (waiting for keys)".into())
        }
        EventContent::Redacted => Some("Message deleted".into()),
        EventContent::Other { description } => Some(format!("sent a {description}")),
        _ => None,
    }
}

/// The part of the message `event` that is the message it replies to.
fn reply_key(event: &str) -> String {
    format!("{event}/reply")
}

/// The id under which the reply preview of part `key` reports the text it
/// shows, cut to fit (see [`crate::selectable::Selectable::id`]).
pub fn shown_id(key: &str) -> Id {
    Id::from(format!("shown:{key}"))
}

impl Part<'_> {
    /// Its text, in units; a reply's preview as `shown` says, if it does.
    pub fn units(&self, shown: &Shown) -> Vec<Unit> {
        let unit = |text: String, prefix: &str, gap| Unit {
            text,
            prefix: prefix.into(),
            gap,
        };
        match &self.text {
            // Copied as the name and time put before messages are.
            Text::Sender(event) => vec![
                unit(display_name(event), "", ""),
                unit(time(event), "", ", "),
            ],
            Text::Reply(reply) => {
                let (name, preview) = reply_text(reply);
                let preview = shown.get(&self.key).cloned().unwrap_or(preview);
                vec![unit(name, "> ", ""), unit(preview, "", ": ")]
            }
            Text::Message(document) => message::text_map(document).units,
            Text::Line(line) => vec![unit(line.clone(), "", "")],
        }
    }
}

/// The parts of each of `rows`.
pub fn parts<'a>(rows: &[Row<'a>]) -> Vec<Vec<Part<'a>>> {
    rows.iter().map(Row::parts).collect()
}

/// The parts of all rows (see [`parts`]) in reading order, each with its
/// row.
pub fn in_order<'p, 'a>(parts: &'p [Vec<Part<'a>>]) -> Vec<(usize, &'p Part<'a>)> {
    (parts.iter().enumerate())
        .flat_map(|(row, parts)| parts.iter().map(move |part| (row, part)))
        .collect()
}

/// Each part's place in reading order, by key, for ordering a selection.
pub fn order<'p>(parts: &'p [Vec<Part<'_>>]) -> HashMap<&'p str, usize> {
    (in_order(parts).into_iter().enumerate())
        .map(|(i, (_, part))| (part.key.as_str(), i))
        .collect()
}

/// The text of the parts of `rows` (see [`parts`]) from the start of
/// `bounds` to its end, as shown (`shown` says what cut texts show). Parts
/// of a row are a line apart, rows a blank line. With several messages,
/// each sender's name and time go before their messages, unless their name
/// is in it already.
pub fn copied(
    rows: &[Row<'_>],
    parts: &[Vec<Part<'_>>],
    bounds: Bounds,
    shown: &Shown,
) -> Option<String> {
    let parts = in_order(parts);
    let texts: Vec<_> = (bounds.start.0..=bounds.end.0)
        .filter_map(|i| {
            let (row, part) = parts.get(i)?;
            let (from, to) = bounds.in_message(i)?;
            let text = units_text(&part.units(shown), from, to);
            (!text.is_empty()).then_some((*row, *part, text))
        })
        .collect();
    let mut messages: Vec<usize> = (texts.iter())
        .filter(|(row, ..)| rows[*row].event().is_some())
        .map(|(row, ..)| *row)
        .collect();
    messages.dedup();
    let several = messages.len() > 1;
    let mut out = String::new();
    let mut sender = None;
    let mut last_row = None;
    for (row, part, text) in texts {
        if let Some(last) = last_row {
            out.push_str(if last == row { "\n" } else { "\n\n" });
        }
        match (&part.text, rows[row].event()) {
            (Text::Sender(event), _) => sender = Some(&event.sender),
            (_, Some(event)) if several && sender != Some(&event.sender) => {
                out.push_str(&format!("{}, {}\n", display_name(event), time(event)));
                sender = Some(&event.sender);
            }
            _ => {}
        }
        out.push_str(&text);
        last_row = Some(row);
    }
    (!out.is_empty()).then_some(out)
}

/// The name of the sender of a reply's message and the start of it, as
/// shown before it is cut to fit.
fn reply_text(reply: &Reply) -> (String, String) {
    let sender = (reply.sender_name.as_deref())
        .or(reply.sender.as_ref().map(|s| s.localpart()))
        .unwrap_or("…");
    let preview = reply.preview.as_deref().unwrap_or(if reply.failed {
        "Could not load the message"
    } else {
        "Loading…"
    });
    (sender.to_owned(), preview.to_owned())
}

pub fn display_name(event: &Event) -> String {
    event
        .sender_name
        .clone()
        .unwrap_or_else(|| event.sender.localpart().to_owned())
}

pub fn time(event: &Event) -> String {
    DateTime::<Local>::from(event.timestamp)
        .format("%H:%M")
        .to_string()
}

#[cfg(test)]
mod tests {
    use mtrx_core::{MembershipChange, OwnedUserId};

    use super::*;
    use crate::selection::{Point, Pos, Selection};

    fn user(name: &str) -> OwnedUserId {
        format!("@{name}:x").try_into().unwrap()
    }

    /// A message from `sender`, `minutes` after the epoch.
    fn message(id: &str, sender: &str, minutes: u64, body: &str) -> Event {
        Event {
            timestamp: SystemTime::UNIX_EPOCH + Duration::from_secs(minutes * 60),
            ..Event::test_message(id, user(sender), body)
        }
    }

    /// A day; alice twice, the second a reply; dave joins; bob, then his
    /// message deleted; alice again much later.
    fn entries() -> Vec<TimelineEntry> {
        let mut reply = message("$b", "alice", 1, "Second");
        if let EventContent::Message(m) = &mut reply.content {
            m.in_reply_to = Some(Reply {
                event_id: "$x:x".try_into().unwrap(),
                sender: Some(user("carol")),
                sender_name: None,
                preview: Some("Where is the data from?".into()),
                kind: None,
                failed: false,
            });
        }
        let joined = Event {
            content: EventContent::Membership {
                user: user("dave"),
                name: None,
                change: MembershipChange::Joined,
            },
            ..message("$s", "dave", 2, "")
        };
        let deleted = Event {
            content: EventContent::Redacted,
            ..message("$d", "bob", 4, "")
        };
        [
            TimelineEntry::DayDivider(SystemTime::UNIX_EPOCH),
            TimelineEntry::Event(message("$a", "alice", 0, "First")),
            TimelineEntry::Event(reply),
            TimelineEntry::Event(joined),
            TimelineEntry::Event(message("$c", "bob", 3, "Third")),
            TimelineEntry::Event(deleted),
            TimelineEntry::Event(message("$e", "alice", 20, "Fourth")),
        ]
        .into()
    }

    fn pos(unit: usize, offset: usize) -> Pos {
        Pos { unit, offset }
    }

    /// The text copied from `anchor` to `focus` (parts and places).
    fn copy(
        entries: &[TimelineEntry],
        anchor: (&str, Pos),
        focus: (&str, Pos),
        shown: &Shown,
    ) -> String {
        let rows = rows(entries);
        let parts = parts(&rows);
        let point = |(event, pos): (&str, Pos)| Point {
            event: event.into(),
            pos,
        };
        let selection = Selection {
            anchor: point(anchor),
            focus: point(focus),
            dragging: false,
        };
        let bounds = selection.ordered(&order(&parts)).unwrap();
        copied(&rows, &parts, bounds, shown).unwrap_or_default()
    }

    #[test]
    fn rows_group_messages_under_their_sender() {
        let entries = entries();
        let rows = rows(&entries);
        let parts = parts(&rows);
        let in_order = in_order(&parts);
        let keys: Vec<&str> = in_order.iter().map(|(_, part)| part.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "day/0",
                "$a/sender",
                "$a",
                "$b/reply",
                "$b",
                "$s/state",
                "$c/sender",
                "$c",
                "$d",
                "$e/sender",
                "$e"
            ]
        );
        let line = |key: &str| {
            let (_, part) = in_order.iter().find(|(_, part)| part.key == key).unwrap();
            part.units(&Shown::new()).remove(0).text
        };
        assert_eq!(line("$s/state"), "dave joined");
        assert_eq!(line("$d"), "Message deleted");
    }

    #[test]
    fn copies_names_once_and_quotes_as_shown() {
        let entries = entries();
        let at = |id: &str| {
            (entries.iter())
                .find_map(|entry| match entry {
                    TimelineEntry::Event(event) if event.id == id => Some(time(event)),
                    _ => None,
                })
                .unwrap()
        };
        let none = Shown::new();
        // From inside alice's first message: her name and time put first;
        // bob's are selected, so not put again.
        assert_eq!(
            copy(&entries, ("$a", pos(0, 2)), ("$c", pos(0, 3)), &none),
            format!(
                "alice, {}\nrst\n\n> carol: Where is the data from?\nSecond\n\n\
                 dave joined\n\nbob, {}\nThi",
                at("$a"),
                at("$c")
            )
        );
        // From her name: as selected.
        assert_eq!(
            copy(&entries, ("$a/sender", pos(0, 0)), ("$a", Pos::END), &none),
            format!("alice, {}\nFirst", at("$a"))
        );
        // Within one message, or a line: no name.
        assert_eq!(
            copy(&entries, ("$a", pos(0, 1)), ("$a", Pos::END), &none),
            "irst"
        );
        assert_eq!(
            copy(&entries, ("$d", pos(0, 8)), ("$d", Pos::END), &none),
            "deleted"
        );
        // The quote as cut to fit.
        let shown = Shown::from([("$b/reply".to_owned(), "Where is…".to_owned())]);
        assert_eq!(
            copy(&entries, ("$b/reply", pos(0, 0)), ("$b", Pos::END), &shown),
            "> carol: Where is…\nSecond"
        );
    }
}
