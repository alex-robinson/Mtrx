//! An open room: timeline and composer.

use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

use chrono::{DateTime, Local};
use iced::{
    Alignment, Element, Length, Padding, Theme,
    keyboard::{Key, key::Named},
    widget::{
        button, column, container, image, mouse_area, row, rule, scrollable, scrollable::Viewport,
        space, stack, text, text_editor,
    },
};
use mtrx_core::{
    CallService, Event, EventContent, MediaKind, MediaRef, MembershipChange, MessageKind, MxcUri,
    OwnedRoomId, OwnedUserId, RoomSummary, RoomTimeline, SendState, TimelineEntry,
};

use crate::{
    actions::{self, Compose, Target},
    app::Message,
    avatar::{self, Avatars, Shape},
    calls::{self, RoomCalls},
    dialog::Kind,
    ellipsis::ellipsis,
    fonts,
    math::{self, Math, Rendered},
    media::{self, Previews},
    members::Members,
    message,
    send_later::{self, SendLater},
    style::{
        self, GUTTER_SPACING, HEADER_AVATAR, MESSAGE_PADDING, SENDER_AVATAR, TIMELINE_PADDING,
    },
};

/// Entries rendered when a room opens; laying out every loaded entry gets
/// slow in big rooms, so only a window at the bottom is drawn.
const WINDOW: usize = 80;
/// Entries added to the window when scrolling up.
const WINDOW_STEP: usize = 50;

/// Consecutive messages of one sender within this gap share a header.
const GROUP_GAP: Duration = Duration::from_secs(5 * 60);

/// Left edge of message text within the timeline column.
const TEXT_INDENT: f32 = MESSAGE_PADDING + SENDER_AVATAR + GUTTER_SPACING;

pub struct Room {
    pub id: OwnedRoomId,
    pub timeline: Arc<RoomTimeline>,
    pub entries: Vec<TimelineEntry>,
    pub composer: text_editor::Content,
    pub later: SendLater,
    pub calls: RoomCalls,
    /// The room menu (invite, edit, leave) is open.
    pub menu: bool,
    pub compose: Compose,
    /// The timeline row whose reaction picker is open.
    pub picker: Option<String>,
    /// The timeline row whose deletion awaits confirmation.
    pub confirm_delete: Option<String>,
    /// Names of others typing.
    pub typing: Vec<String>,
    /// The member list, while shown.
    pub members: Option<Members>,
    /// How many of the newest entries are rendered.
    shown: usize,
    pub paginating: bool,
    pub at_start: bool,
    pub error: Option<String>,
    /// [`Event::id`] of the message under the cursor.
    hovered: Option<String>,
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
            calls: RoomCalls::new(id.clone()),
            menu: false,
            compose: Compose::New,
            picker: None,
            confirm_delete: None,
            typing: Vec::new(),
            members: None,
            id,
            entries: timeline.current(),
            timeline,
            composer: text_editor::Content::new(),
            later: SendLater::default(),
            shown: WINDOW,
            paginating: false,
            at_start: false,
            error: None,
            hovered: None,
        }
    }

    /// The cursor entered or left the message with [`Event::id`] `id`.
    pub fn hover(&mut self, id: String, entered: bool) {
        if entered {
            self.hovered = Some(id);
        } else if self.hovered.as_ref() == Some(&id) {
            // Entering the next message may have been reported first.
            self.hovered = None;
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

    /// Pictures of the header and the rendered senders, with their sizes.
    pub fn avatars<'a>(
        &'a self,
        summary: Option<&'a RoomSummary>,
    ) -> impl Iterator<Item = (&'a MxcUri, f32)> {
        let room = summary.and_then(|s| Some((s.avatar.as_deref()?, HEADER_AVATAR)));
        let senders = self.visible().iter().filter_map(|e| match e {
            TimelineEntry::Event(event) => Some((event.sender_avatar.as_deref()?, SENDER_AVATAR)),
            _ => None,
        });
        let members = self.members.iter().flat_map(crate::members::avatars);
        room.into_iter().chain(senders).chain(members)
    }

    /// Images of the rendered messages, for loading their previews.
    pub fn images(&self) -> impl Iterator<Item = &MediaRef> {
        self.visible().iter().filter_map(|e| match e {
            TimelineEntry::Event(Event {
                content: EventContent::Message(m),
                ..
            }) => m
                .media
                .as_ref()
                .filter(|media| media.kind == MediaKind::Image)
                .map(|media| &media.source),
            _ => None,
        })
    }

    pub fn view<'a>(
        &'a self,
        summary: Option<&'a RoomSummary>,
        look: &Look<'a>,
        scheduled: &send_later::Pending<'a>,
    ) -> Element<'a, Message> {
        let header = self.header(summary, look);
        let context = Context {
            theme: &look.theme,
            math: look.math,
            params: &look.params,
            avatars: look.avatars,
            previews: look.previews,
            colored_names: look.colored_names,
            call_services: look.call_services,
            hovered: self.hovered.as_deref(),
            picker: self.picker.as_deref(),
            confirm_delete: self.confirm_delete.as_deref(),
            composer_empty: self.composer.text().trim().is_empty(),
        };

        let mut timeline = column![].padding([8.0, TIMELINE_PADDING]);
        let all_shown = self.shown >= self.entries.len();
        if self.at_start && all_shown {
            timeline = timeline.push(notice("This is the start of the room."));
        } else if self.paginating || !all_shown {
            timeline = timeline.push(notice("Loading…"));
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
                    timeline = timeline.push(event_view(event, !grouped, &context));
                    previous = matches!(event.content, EventContent::Message(_)).then_some(event);
                }
                TimelineEntry::DayDivider(day) => {
                    timeline = timeline.push(day_divider(*day));
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

        let cancellable = self.compose.cancellable();
        let composer = text_editor(&self.composer)
            .style(style::editor)
            .placeholder("Message…  (Markdown, $…$ for math; Shift+Enter for a new line)")
            .on_action(Message::Composer)
            .key_binding(move |press| match press.key {
                Key::Named(Named::Enter) if !press.modifiers.shift() => {
                    Some(text_editor::Binding::Custom(Message::Send))
                }
                Key::Named(Named::Escape) if cancellable => Some(text_editor::Binding::Custom(
                    Message::Act(actions::Message::CancelCompose),
                )),
                _ => text_editor::Binding::from_key_press(press),
            })
            .padding(10)
            .max_height(200);
        // An edit of a sent message cannot be scheduled.
        let later = (!matches!(self.compose, Compose::Edit(_))).then(|| {
            button(
                text(if self.compose.scheduled().is_some() {
                    "Change time…"
                } else {
                    "Send later…"
                })
                .size(13),
            )
            .padding([10, 12])
            .style(style::secondary_button)
            .on_press(Message::Later(send_later::Message::TogglePicker))
        });

        let composer_empty = self.composer.text().trim().is_empty();
        let mut bottom = column![]
            .spacing(6)
            .padding([8.0, TIMELINE_PADDING + MESSAGE_PADDING]);
        if !scheduled.messages.is_empty() {
            let editing = self.compose.scheduled().map(|m| m.id.as_str());
            bottom = bottom.push(send_later::queue(scheduled, editing, composer_empty));
        }
        if later.is_some()
            && let Some(picker) = send_later::picker(&self.later, !composer_empty, scheduled.now)
        {
            bottom = bottom.push(picker);
        }
        if let Some(typing) = actions::typing(&self.typing) {
            bottom = bottom.push(typing);
        }
        if let Some(bar) = self.compose.bar() {
            bottom = bottom.push(bar);
        }
        let attach = button(text("Attach…").size(13))
            .padding([10, 12])
            .style(style::secondary_button)
            .on_press(Message::Attach);
        let mut input = row![composer, attach].spacing(8).align_y(Alignment::End);
        if let Some(later) = later {
            input = input.push(later);
        }
        bottom = bottom.push(input);
        if let Some(error) = &self.error {
            bottom = bottom.push(text(error).size(12).style(text::danger));
        }

        let main = column![timeline, bottom].width(Length::Fill);
        let body: Element<'a, Message> = match &self.members {
            Some(members) => {
                let name = summary.map_or("", |s| s.name.as_str());
                row![
                    main,
                    members.view(&self.id, name, look.own_user, look.avatars)
                ]
                .into()
            }
            None => main.into(),
        };
        column![header, body].into()
    }

    /// Invite, edit name and topic, leave.
    fn menu_view<'a>(&self, summary: Option<&'a RoomSummary>) -> Element<'a, Message> {
        let name = summary.map_or_else(String::new, |s| s.name.clone());
        let item = |label, kind| {
            button(text(label).size(13))
                .padding([4, 10])
                .style(style::secondary_button)
                .on_press(Message::OpenDialog(kind))
        };
        container(
            row![
                item(
                    "Invite people…",
                    Kind::Invite {
                        room: self.id.clone(),
                        name: name.clone(),
                    }
                ),
                item(
                    "Name and topic…",
                    Kind::Edit {
                        room: self.id.clone(),
                        name: name.clone(),
                        topic: summary.and_then(|s| s.topic.clone()).unwrap_or_default(),
                    }
                ),
                space::horizontal(),
                item(
                    "Leave room…",
                    Kind::Leave {
                        room: self.id.clone(),
                        name,
                    }
                ),
            ]
            .spacing(6),
        )
        .padding([8, 12])
        .width(Length::Fill)
        .style(style::code_block)
        .into()
    }

    /// Picture, name and topic, the call controls, and their panel when open.
    fn header<'a>(
        &'a self,
        summary: Option<&'a RoomSummary>,
        look: &Look<'a>,
    ) -> Element<'a, Message> {
        let avatars = look.avatars;
        let name = summary.map_or("", |s| s.name.as_str());
        let mut title = column![ellipsis(name).size(16).font(fonts::SEMIBOLD)];
        // A room just created or joined is listed once sync delivers it.
        if summary.is_none() {
            title = title.push(
                text("Setting up the room… it appears in the list in a moment.")
                    .size(12)
                    .style(text::secondary),
            );
        }
        if let Some(topic) = summary
            .and_then(|s| s.topic.as_deref())
            .and_then(|t| t.lines().next())
        {
            title = title.push(ellipsis(topic).size(12).style(text::secondary));
        }
        let avatar = avatars.view(
            self.id.as_str(),
            name,
            summary.and_then(|s| s.avatar.as_deref()),
            HEADER_AVATAR,
            Shape::Circle,
        );
        let mut header = column![
            row![
                avatar,
                title.width(Length::Fill),
                self.calls.header_controls(look.call_services),
                button(text("Members").size(13))
                    .padding([5, 10])
                    .style(style::list_item(self.members.is_some()))
                    .on_press(Message::ToggleMembers),
                button(text("⋯").size(16))
                    .padding([2, 10])
                    .style(style::list_item(self.menu))
                    .on_press(Message::RoomMenu(!self.menu)),
            ]
            .spacing(10)
            .align_y(Alignment::Center)
            .padding([8.0, TIMELINE_PADDING + MESSAGE_PADDING]),
        ];
        if self.menu {
            header = header.push(
                container(self.menu_view(summary))
                    .padding(Padding::from([0.0, TIMELINE_PADDING + MESSAGE_PADDING]).bottom(8.0)),
            );
        }
        if let Some(panel) = self.calls.panel(look.call_services) {
            header = header.push(
                container(panel)
                    .padding(Padding::from([0.0, TIMELINE_PADDING + MESSAGE_PADDING]).bottom(8.0)),
            );
        }
        header
            .push(rule::horizontal(1).style(style::separator))
            .into()
    }
}

/// What rendering a room needs from the rest of the app.
pub struct Look<'a> {
    pub theme: Theme,
    pub math: &'a Math,
    pub params: math::Params,
    pub avatars: &'a Avatars,
    /// Senders' names in their colour instead of the text colour.
    pub colored_names: bool,
    /// Our call services, to recognise and start calls.
    pub call_services: &'a [CallService],
    pub own_user: &'a OwnedUserId,
    pub previews: &'a Previews,
}

/// What rendering an event needs besides the event.
struct Context<'a, 'b> {
    theme: &'b Theme,
    math: &'a Math,
    params: &'b math::Params,
    avatars: &'b Avatars,
    previews: &'b Previews,
    colored_names: bool,
    call_services: &'b [CallService],
    hovered: Option<&'b str>,
    picker: Option<&'b str>,
    confirm_delete: Option<&'b str>,
    composer_empty: bool,
}

fn event_view<'a>(
    event: &'a Event,
    with_header: bool,
    context: &Context<'a, '_>,
) -> Element<'a, Message> {
    let Context {
        theme,
        math,
        params,
        ..
    } = *context;
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
                (kind, _) => match &m.media {
                    Some(file) => media::view(file, &m.body, context.previews),
                    None => text(format!("[{kind:?}] {}", m.body))
                        .style(text::secondary)
                        .into(),
                },
            };
            let mut parts = column![].spacing(4);
            if let Some(reply) = &m.in_reply_to {
                parts = parts.push(actions::quote(reply));
            }
            parts = parts.push(content);
            if let Some(cards) = calls::cards(&m.document.links(), context.call_services) {
                parts = parts.push(cards);
            }
            if m.edited {
                parts = parts.push(text("(edited)").size(11).style(text::secondary));
            }
            if !m.reactions.is_empty() {
                parts = parts.push(actions::reactions(event, &m.reactions));
            }
            parts.into()
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

    let mut item = column![].spacing(2).width(Length::Fill);
    if with_header {
        item = item.push(sender_line(event, context.colored_names));
    }
    item = item.push(body);
    let message = match &event.content {
        EventContent::Message(m) => Some(m),
        _ => None,
    };
    if let Some(m) = message
        && context.picker == Some(event.id.as_str())
    {
        item = item.push(actions::picker(event, &m.reactions));
    }
    if context.confirm_delete == Some(event.id.as_str()) {
        item = item.push(actions::confirm_delete(event));
    }
    if let Some(markers) = actions::read_markers(event, context.avatars) {
        item = item.push(markers);
    }
    match &event.send_state {
        SendState::Sent => {}
        SendState::Sending => item = item.push(text("sending…").size(11).style(text::secondary)),
        SendState::Failed(e) => {
            item = item.push(text(format!("not sent: {e}")).size(11).style(text::danger))
        }
    }

    // The avatar starts a group; follow-ups show their time there on hover.
    let hovered = context.hovered == Some(event.id.as_str());
    let gutter: Element<'a, Message> = if with_header {
        context.avatars.view(
            event.sender.as_str(),
            &display_name(event),
            event.sender_avatar.as_deref(),
            SENDER_AVATAR,
            Shape::Circle,
        )
    } else if hovered {
        container(text(time(event)).size(10).style(text::secondary))
            .center_x(SENDER_AVATAR)
            .padding(Padding::ZERO.top(5))
            .into()
    } else {
        space().width(SENDER_AVATAR).into()
    };
    let top = if with_header { 4.0 } else { 1.0 };
    let row = container(row![gutter, item].spacing(GUTTER_SPACING))
        .padding(Padding::new(MESSAGE_PADDING).top(top).bottom(1))
        .width(Length::Fill)
        .style(style::message_row(hovered));
    // Actions float over the top right of a hovered message.
    let row: Element<'a, Message> = match message {
        Some(m) if hovered && event.event_id.is_some() => {
            let preview = first_line(&m.document.to_plain_text());
            let target = Target::of(event, display_name(event), preview);
            // Editing replaces the composer's text, so only from an empty one.
            let edit_text = context.composer_empty.then(|| m.body.clone());
            stack![
                row,
                container(actions::toolbar(event, target, edit_text))
                    .align_right(Length::Fill)
                    .padding(Padding::ZERO.top(2.0).right(8.0)),
            ]
            .into()
        }
        _ => row.into(),
    };
    let row = mouse_area(row)
        .on_enter(Message::Hover(event.id.clone(), true))
        .on_exit(Message::Hover(event.id.clone(), false));
    if with_header {
        // Space between groups, outside the hover background.
        column![space().height(6), row].into()
    } else {
        row.into()
    }
}

/// Sender name, in their colour if `colored`, and time above the first
/// message of a group.
fn sender_line<'a>(event: &Event, colored: bool) -> Element<'a, Message> {
    let mut name = text(display_name(event)).font(fonts::SEMIBOLD);
    if colored {
        let sender = event.sender.clone();
        name = name.style(move |theme| text::Style {
            color: Some(avatar::name_color(sender.as_str(), theme)),
        });
    }
    row![name, text(time(event)).size(11).style(text::secondary)]
        .spacing(8)
        .align_y(Alignment::End)
        .into()
}

fn time(event: &Event) -> String {
    DateTime::<Local>::from(event.timestamp)
        .format("%H:%M")
        .to_string()
}

/// A muted line in the text column, e.g. "Loading…".
fn notice<'a>(content: impl text::IntoFragment<'a>) -> Element<'a, Message> {
    container(text(content).size(12).style(text::secondary))
        .padding(Padding::new(4.0).left(TEXT_INDENT))
        .into()
}

/// The date between two days' messages, centred on a line.
fn day_divider<'a>(day: SystemTime) -> Element<'a, Message> {
    let day: DateTime<Local> = day.into();
    row![
        rule::horizontal(1).style(style::separator),
        text(day.format("%A, %e %B %Y").to_string())
            .size(12)
            .style(text::secondary),
        rule::horizontal(1).style(style::separator),
    ]
    .spacing(12)
    .align_y(Alignment::Center)
    .padding([10.0, MESSAGE_PADDING])
    .into()
}

/// The first non-empty line of a message, for previews.
fn first_line(body: &str) -> String {
    body.lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or_default()
        .to_owned()
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
    notice(state_summary(events))
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
