//! An open room: timeline and composer.

use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

use chrono::{DateTime, Local};
use iced::{
    Alignment, Element, Length, Theme,
    keyboard::{Key, key::Named},
    widget::{
        button, column, container, image, row, scrollable, scrollable::Viewport, space, text,
        text_editor,
    },
};
use mtrx_core::{
    Event, EventContent, MembershipChange, MessageKind, OwnedRoomId, RoomTimeline,
    ScheduledMessage, SendState, TimelineEntry,
};

use crate::{
    app::Message,
    fonts,
    math::{self, Math, Rendered},
    message,
    send_later::{self, SendLater},
};

/// Entries rendered when a room opens; laying out every loaded entry gets
/// slow in big rooms, so only a window at the bottom is drawn.
const WINDOW: usize = 80;
/// Entries added to the window when scrolling up.
const WINDOW_STEP: usize = 50;

/// Consecutive messages of one sender within this gap share a header.
const GROUP_GAP: Duration = Duration::from_secs(5 * 60);

pub struct Room {
    pub id: OwnedRoomId,
    pub timeline: Arc<RoomTimeline>,
    pub entries: Vec<TimelineEntry>,
    pub composer: text_editor::Content,
    pub later: SendLater,
    /// How many of the newest entries are rendered.
    shown: usize,
    pub paginating: bool,
    pub at_start: bool,
    pub error: Option<String>,
}

/// What scrolling to the top should do.
pub enum Older {
    None,
    /// Render more of the already loaded entries.
    ShowMore,
    /// Load older events from the server.
    Paginate,
}

impl Room {
    pub fn new(id: OwnedRoomId, timeline: Arc<RoomTimeline>) -> Self {
        Self {
            id,
            entries: timeline.current(),
            timeline,
            composer: text_editor::Content::new(),
            later: SendLater::default(),
            shown: WINDOW,
            paginating: false,
            at_start: false,
            error: None,
        }
    }

    /// Reaction to a scroll: near the top (the view is anchored to the
    /// bottom, so the top is relative offset 1) we want older entries.
    pub fn older(&self, viewport: &Viewport) -> Older {
        if viewport.relative_offset().y < 0.9 {
            Older::None
        } else if self.shown < self.entries.len() {
            Older::ShowMore
        } else if !self.paginating && !self.at_start {
            Older::Paginate
        } else {
            Older::None
        }
    }

    pub fn show_more(&mut self) {
        self.shown += WINDOW_STEP;
    }

    fn visible(&self) -> &[TimelineEntry] {
        &self.entries[self.entries.len().saturating_sub(self.shown)..]
    }

    /// Rendered messages with math, for typesetting.
    pub fn math_documents(&self) -> impl Iterator<Item = &mtrx_format::Document> {
        self.visible().iter().filter_map(|e| match e {
            TimelineEntry::Event(Event {
                content: EventContent::Message(m),
                ..
            }) if m.document.has_math() => Some(&m.document),
            _ => None,
        })
    }

    pub fn view<'a>(
        &'a self,
        name: &'a str,
        theme: &Theme,
        math: &'a Math,
        params: &math::Params,
        scheduled: &[&'a ScheduledMessage],
        now: SystemTime,
    ) -> Element<'a, Message> {
        let header = container(text(name).size(18).font(fonts::SEMIBOLD)).padding([12, 24]);

        let mut timeline = column![].spacing(2).padding([12, 24]);
        let all_shown = self.shown >= self.entries.len();
        if self.at_start && all_shown {
            timeline = timeline.push(
                text("This is the start of the room.")
                    .size(12)
                    .style(text::secondary),
            );
        } else if self.paginating || !all_shown {
            timeline = timeline.push(text("Loading…").size(12).style(text::secondary));
        }
        let mut previous: Option<&Event> = None;
        // Consecutive state events (joins, settings) collapse into one line.
        let mut state: Vec<&Event> = Vec::new();
        for entry in self.visible() {
            if let TimelineEntry::Event(event) = entry
                && is_state(event)
            {
                state.push(event);
                continue;
            }
            if !state.is_empty() {
                timeline = timeline.push(state_line(&std::mem::take(&mut state)));
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
                    timeline = timeline.push(event_view(event, !grouped, theme, math, params));
                    previous = matches!(event.content, EventContent::Message(_)).then_some(event);
                }
                TimelineEntry::DayDivider(day) => {
                    let day: DateTime<Local> = (*day).into();
                    timeline = timeline.push(
                        container(
                            text(day.format("%A, %e %B %Y").to_string())
                                .size(12)
                                .style(text::secondary),
                        )
                        .center_x(Length::Fill)
                        .padding([10, 0]),
                    );
                    previous = None;
                }
                TimelineEntry::ReadMarker => {}
            }
        }
        if !state.is_empty() {
            timeline = timeline.push(state_line(&state));
        }
        let timeline = scrollable(timeline)
            .anchor_bottom()
            .on_scroll(Message::Scrolled)
            .height(Length::Fill)
            .width(Length::Fill);

        let editing = self.later.editing.is_some();
        let composer = text_editor(&self.composer)
            .placeholder("Message…  (Markdown, $…$ for math; Shift+Enter for a new line)")
            .on_action(Message::Composer)
            .key_binding(move |press| match press.key {
                Key::Named(Named::Enter) if !press.modifiers.shift() => {
                    Some(text_editor::Binding::Custom(Message::Send))
                }
                Key::Named(Named::Escape) if editing => Some(text_editor::Binding::Custom(
                    Message::Later(send_later::Message::CancelEdit),
                )),
                _ => text_editor::Binding::from_key_press(press),
            })
            .padding(10)
            .max_height(200);
        let later = button(
            text(if editing {
                "Change time…"
            } else {
                "Send later…"
            })
            .size(13),
        )
        .padding([10, 12])
        .style(button::secondary)
        .on_press(Message::Later(send_later::Message::TogglePicker));

        let composer_empty = self.composer.text().trim().is_empty();
        let mut bottom = column![].spacing(6).padding([8, 24]);
        if !scheduled.is_empty() {
            bottom = bottom.push(send_later::queue(
                scheduled,
                &self.later,
                composer_empty,
                now,
            ));
        }
        if let Some(picker) = send_later::picker(&self.later, !composer_empty, now) {
            bottom = bottom.push(picker);
        }
        if editing {
            bottom = bottom.push(
                row![
                    text("Editing a scheduled message: Enter saves, Esc cancels.")
                        .size(12)
                        .style(text::secondary),
                    button(text("Cancel editing").size(12))
                        .padding([2, 8])
                        .style(button::text)
                        .on_press(Message::Later(send_later::Message::CancelEdit)),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }
        bottom = bottom.push(row![composer, later].spacing(8).align_y(Alignment::End));
        if let Some(error) = &self.error {
            bottom = bottom.push(text(error).size(12).style(text::danger));
        }

        column![header, timeline, bottom].into()
    }
}

fn event_view<'a>(
    event: &'a Event,
    with_header: bool,
    theme: &Theme,
    math: &'a Math,
    params: &math::Params,
) -> Element<'a, Message> {
    let time = DateTime::<Local>::from(event.timestamp)
        .format("%H:%M")
        .to_string();
    let name = display_name(event);

    let body: Element<'a, Message> = match &event.content {
        EventContent::Message(m) => {
            let content: Element<'a, Message> = match (m.kind, m.document.has_math()) {
                (MessageKind::Text | MessageKind::Notice | MessageKind::Emote, true) => {
                    match math.get(&Math::key(&m.document, params)) {
                        Some(Rendered::Image {
                            handle,
                            width,
                            height,
                        }) => image(handle.clone()).width(*width).height(*height).into(),
                        _ => message::view(&m.document, theme, Message::LinkClicked),
                    }
                }
                (MessageKind::Text | MessageKind::Notice | MessageKind::Emote, false) => {
                    message::view(&m.document, theme, Message::LinkClicked)
                }
                (kind, _) => text(format!("[{kind:?}] {}", m.body))
                    .style(text::secondary)
                    .into(),
            };
            let mut extras = row![].spacing(6);
            if m.edited {
                extras = extras.push(text("(edited)").size(11).style(text::secondary));
            }
            for (key, count) in &m.reactions {
                extras = extras.push(
                    container(text(format!("{key} {count}")).size(12))
                        .padding([1, 6])
                        .style(container::rounded_box),
                );
            }
            column![content, extras].spacing(2).into()
        }
        EventContent::UnableToDecrypt => text("Unable to decrypt this message (waiting for keys)")
            .size(13)
            .style(text::secondary)
            .into(),
        EventContent::Redacted => text("Message deleted")
            .size(13)
            .style(text::secondary)
            .into(),
        EventContent::Other { description } => text(format!("sent a {description}"))
            .size(13)
            .style(text::secondary)
            .into(),
        // Rendered as summaries by `state_line`.
        EventContent::Membership { .. }
        | EventContent::DisplayNameChange { .. }
        | EventContent::OtherState { .. } => return space().into(),
    };

    let state: Element<'a, Message> = match &event.send_state {
        SendState::Sent => space().into(),
        SendState::Sending => text("sending…").size(11).style(text::secondary).into(),
        SendState::Failed(e) => text(format!("not sent: {e}"))
            .size(11)
            .style(text::danger)
            .into(),
    };

    let mut item = column![].spacing(2);
    if with_header {
        item = item.push(
            row![
                text(name).font(fonts::SEMIBOLD),
                text(time).size(11).style(text::secondary)
            ]
            .spacing(8)
            .align_y(Alignment::End),
        );
    }
    item = item.push(body).push(state);
    container(item)
        .padding(if with_header { [8, 0] } else { [1, 0] })
        .width(Length::Fill)
        .into()
}

fn display_name(event: &Event) -> String {
    event
        .sender_name
        .clone()
        .unwrap_or_else(|| event.sender.localpart().to_owned())
}

fn is_state(event: &Event) -> bool {
    matches!(
        event.content,
        EventContent::Membership { .. }
            | EventContent::DisplayNameChange { .. }
            | EventContent::OtherState { .. }
    )
}

fn state_line<'a>(events: &[&Event]) -> Element<'a, Message> {
    container(text(state_summary(events)).size(12).style(text::secondary))
        .padding([4, 0])
        .into()
}

/// One line for a run of state events, e.g. "alice created and configured
/// the room · alice invited bob · bob joined".
fn state_summary(events: &[&Event]) -> String {
    let creators: Vec<_> = events
        .iter()
        .filter(|e| matches!(&e.content, EventContent::OtherState { event_type } if event_type == "m.room.create"))
        .map(|e| e.sender.clone())
        .collect();
    let mut parts: Vec<String> = Vec::new();
    let mut joined: Vec<String> = Vec::new();
    let mut left: Vec<String> = Vec::new();
    for event in events {
        let name = display_name(event);
        let part = match &event.content {
            EventContent::OtherState { event_type } if event_type == "m.room.create" => {
                format!("{name} created and configured the room")
            }
            // Initial settings belong to "created and configured".
            EventContent::OtherState { .. } if creators.contains(&event.sender) => continue,
            EventContent::OtherState { event_type } => match event_type.as_str() {
                "m.room.name" => format!("{name} renamed the room"),
                "m.room.topic" => format!("{name} changed the topic"),
                "m.room.avatar" => format!("{name} changed the room picture"),
                "m.room.pinned_events" => format!("{name} changed the pinned messages"),
                _ => format!("{name} changed the room settings"),
            },
            EventContent::Membership {
                user,
                name: target,
                change,
            } => {
                let target = target
                    .clone()
                    .unwrap_or_else(|| user.localpart().to_owned());
                match change {
                    MembershipChange::Joined | MembershipChange::InvitationAccepted => {
                        if !creators.contains(user) && !joined.contains(&target) {
                            joined.push(target);
                        }
                        continue;
                    }
                    MembershipChange::Left => {
                        left.push(target);
                        continue;
                    }
                    MembershipChange::Invited => format!("{name} invited {target}"),
                    MembershipChange::InvitationRejected => format!("{target} declined the invite"),
                    MembershipChange::InvitationRevoked => {
                        format!("{name} withdrew the invite for {target}")
                    }
                    MembershipChange::Kicked => format!("{name} removed {target}"),
                    MembershipChange::Banned => format!("{name} banned {target}"),
                    MembershipChange::Unbanned => format!("{name} unbanned {target}"),
                    MembershipChange::Knocked => format!("{target} asked to join"),
                    MembershipChange::Other => continue,
                }
            }
            EventContent::DisplayNameChange { user, new_name } => match new_name {
                Some(new) => format!("{} is now {new}", user.localpart()),
                None => format!("{} removed their display name", user.localpart()),
            },
            _ => continue,
        };
        if parts.last() != Some(&part) {
            parts.push(part);
        }
    }
    if !joined.is_empty() {
        parts.push(format!("{} joined", joined.join(", ")));
    }
    if !left.is_empty() {
        parts.push(format!("{} left", left.join(", ")));
    }
    parts.join(" · ")
}
