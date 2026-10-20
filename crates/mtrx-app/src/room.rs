//! An open room: timeline and composer.

use std::{
    collections::HashMap,
    rc::Rc,
    sync::Arc,
    time::{Duration, SystemTime},
};

use chrono::{DateTime, Local};
use iced::{
    Alignment, Element, Length, Padding, Theme,
    keyboard::{Key, key::Named},
    mouse,
    widget::{
        button, column, container, image, mouse_area, row, rule, scrollable, scrollable::Viewport,
        space, stack, text, text_editor,
    },
};
use mtrx_core::{
    CallService, Event, EventContent, Media, MembershipChange, MessageKind, MxcUri, OwnedRoomId,
    OwnedUserId, RoomKind, RoomSummary, RoomTimeline, ScheduledMessage, SendState, TimelineEntry,
};

use crate::{
    actions::{self, Compose, Editing, InlineEdit, PickerAt, Target, act},
    app::Message,
    avatar::{self, Avatars, Shape},
    calls::{self, RoomCalls},
    context_menu::context_menu,
    drag_scroll::drag_scroll,
    ellipsis::ellipsis,
    emoji, fonts,
    hint::{Position, hint},
    icons, info,
    math::{self, Math, Rendered},
    media::{self, Previews},
    members::Members,
    message,
    message_menu::{self, MenuAt},
    popover::{Edge, Side, popover},
    selectable_image::selectable_image,
    selection::{self, Bounds, Selection, units_text},
    send_later::{self, SendLater},
    style::{
        self, GUTTER_SPACING, HEADER_AVATAR, MESSAGE_PADDING, SENDER_AVATAR, TEXT_MARGIN,
        TIMELINE_PADDING, TOOLBAR_HEIGHT,
    },
};

/// Entries rendered when a room opens; laying out every loaded entry gets
/// slow in big rooms, so only a window at the bottom is drawn.
const WINDOW: usize = 80;
/// Entries added to the window when scrolling up.
const WINDOW_STEP: usize = 50;
/// Older events are loaded until there are this many entries (or the room
/// has no more): fewer may not fill the view, and then there is nothing to
/// scroll up for more.
const FILL: usize = 40;

/// The message field, focused again after inserting an emoji.
pub const COMPOSER: &str = "composer";
/// The timeline's scrollable.
pub const TIMELINE: &str = "timeline";

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
    pub compose: Compose,
    /// A sent or scheduled message being edited in place.
    pub editing: Option<InlineEdit>,
    /// The timeline row whose reaction window is open, and where.
    pub picker: Option<(String, PickerAt)>,
    /// The timeline row whose menu is open, and where.
    pub menu: Option<(String, MenuAt)>,
    /// Text selected in the timeline.
    pub selection: Option<Selection>,
    /// All emoji in the reaction picker, while shown.
    pub emoji: Option<emoji::Panel>,
    /// The composer's emoji panel, while shown.
    pub composer_emoji: Option<emoji::Panel>,
    /// The timeline row whose deletion awaits confirmation.
    pub confirm_delete: Option<String>,
    /// Names of others typing.
    pub typing: Vec<String>,
    /// The member list, while shown.
    pub members: Option<Members>,
    /// How many of the newest entries are rendered.
    shown: usize,
    pub paginating: bool,
    /// A new timeline is being loaded (⌘R).
    pub reloading: bool,
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
            compose: Compose::New,
            editing: None,
            picker: None,
            menu: None,
            selection: None,
            emoji: None,
            composer_emoji: None,
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
            reloading: false,
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
        } else if !self.paginating && !self.at_start() {
            Older::Paginate
        } else {
            Older::None
        }
    }

    /// The scheduled messages changed: one being edited or given a new time
    /// may have been sent (it became due) or cancelled elsewhere.
    pub fn scheduled_changed(&mut self, scheduled: &[ScheduledMessage]) {
        let gone = |id: &str| !scheduled.iter().any(|m| m.id == id);
        if self
            .later
            .rescheduling
            .as_ref()
            .is_some_and(|m| gone(&m.id))
        {
            self.later = Default::default();
        }
        let Some(edit) = &self.editing else {
            return;
        };
        if !matches!(&edit.target, Editing::Scheduled(id) if gone(id)) {
            return;
        }
        // Keep the changes: in the composer if it is free.
        if self.composer.text().trim().is_empty() {
            self.composer = text_editor::Content::with_text(&edit.text());
            self.editing = None;
            self.error = Some(
                "The scheduled message was sent before your changes were saved; \
                 they are in the composer."
                    .into(),
            );
        } else {
            self.error =
                Some("The scheduled message was sent before your changes were saved.".into());
        }
    }

    /// Show a freshly loaded timeline, keeping what is being written.
    pub fn reload(&mut self, timeline: Arc<RoomTimeline>) {
        self.entries = timeline.current();
        self.timeline = timeline;
        self.shown = WINDOW;
        self.paginating = false;
        self.reloading = false;
        self.picker = None;
        self.confirm_delete = None;
        self.error = None;
    }

    /// Back-pagination reached the room's creation.
    pub fn at_start(&self) -> bool {
        matches!(self.entries.first(), Some(TimelineEntry::Start))
    }

    /// Too few entries are loaded to fill the view.
    pub fn wants_older(&self) -> bool {
        !self.paginating && !self.at_start() && self.entries.len() < FILL
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
        let senders = self
            .visible()
            .iter()
            .filter_map(|e| match e {
                TimelineEntry::Event(event) => Some(event),
                _ => None,
            })
            .flat_map(|event| {
                let sender = event.sender_avatar.as_deref().map(|a| (a, SENDER_AVATAR));
                sender.into_iter().chain(actions::reader_avatars(event))
            });
        let members = self.members.iter().flat_map(crate::members::avatars);
        room.into_iter().chain(senders).chain(members)
    }

    /// Images of the rendered messages, for loading their previews.
    /// Files with previews: images and PDFs.
    pub fn previewed(&self) -> impl Iterator<Item = &Media> {
        self.visible().iter().filter_map(|e| match e {
            TimelineEntry::Event(Event {
                content: EventContent::Message(m),
                ..
            }) => m.media.as_ref().filter(|media| media::previewed(media)),
            _ => None,
        })
    }

    /// Each message's index in the timeline, for ordering a selection.
    fn order(&self) -> HashMap<&str, usize> {
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(i, entry)| match entry {
                TimelineEntry::Event(event) => Some((event.id.as_str(), i)),
                _ => None,
            })
            .collect()
    }

    /// The selected text, as shown; with several messages, each sender's
    /// name and time before their messages.
    pub fn selected_text(&self) -> Option<String> {
        let bounds = self.selection.as_ref()?.ordered(&self.order())?;
        let messages: Vec<_> = (bounds.start.0..=bounds.end.0)
            .filter_map(|i| match self.entries.get(i)? {
                TimelineEntry::Event(event) => match &event.content {
                    EventContent::Message(m) => Some((event, m, bounds.in_message(i)?)),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        let several = messages.len() > 1;
        let mut out = String::new();
        let mut sender = None;
        for (event, m, (from, to)) in messages {
            let text = units_text(&message::text_map(&m.document).units, from, to);
            if text.is_empty() {
                continue;
            }
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            if several && sender != Some(&event.sender) {
                out.push_str(&format!("{}, {}\n", display_name(event), time(event)));
                sender = Some(&event.sender);
            }
            out.push_str(&text);
        }
        (!out.is_empty()).then_some(out)
    }

    pub fn view<'a>(
        &'a self,
        summary: Option<&'a RoomSummary>,
        look: &Look<'a>,
        scheduled: &send_later::Pending<'a>,
    ) -> Element<'a, Message> {
        let header = self.header(summary, look);
        let order = self.selection.as_ref().map(|_| self.order());
        let selected = self
            .selection
            .as_ref()
            .zip(order.as_ref())
            .and_then(|(selection, order)| Some((selection.ordered(order)?, order)));
        let context = Context {
            theme: &look.theme,
            math: look.math,
            params: &look.params,
            avatars: look.avatars,
            previews: look.previews,
            colored_names: look.colored_names,
            call_services: look.call_services,
            hovered: self.hovered.as_deref(),
            picker: self.picker.as_ref().map(|(id, at)| (id.as_str(), *at)),
            menu: self.menu.as_ref().map(|(id, at)| (id.as_str(), *at)),
            selected,
            dragging: self.selection.as_ref().is_some_and(|s| s.dragging),
            emoji: self.emoji.as_ref(),
            frequent_emoji: look.frequent_emoji,
            confirm_delete: self.confirm_delete.as_deref(),
            editing: self.editing.as_ref(),
        };

        let mut timeline = column![].padding([8.0, TIMELINE_PADDING]);
        let all_shown = self.shown >= self.entries.len();
        if self.reloading {
            timeline = timeline.push(notice("Reloading…"));
        } else if !(self.at_start() && all_shown) && (self.paginating || !all_shown) {
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
                TimelineEntry::Start => {
                    timeline = timeline.push(notice("This is the start of the room."));
                }
                TimelineEntry::ReadMarker => {}
            }
        }
        if !state.is_empty() {
            timeline = timeline.push(state_line(&state));
        }
        // Pegged to the bottom also when shorter than the view: new messages
        // push older ones up, and loaded history appears above.
        let timeline = scrollable(container(timeline).align_bottom(Length::Shrink))
            .id(TIMELINE)
            .anchor_bottom()
            .on_scroll(Message::Scrolled)
            .height(Length::Fill)
            .width(Length::Fill);
        // A click beside the text clears the selection; dragging one past
        // the top or bottom scrolls.
        let dragging = self.selection.as_ref().is_some_and(|s| s.dragging);
        let timeline = drag_scroll(
            mouse_area(timeline).on_press(Message::Select(selection::Message::Clear)),
            dragging,
            Message::ScrollTimeline,
        );

        let cancellable = self.compose.cancellable();
        let composer = text_editor(&self.composer)
            .id(COMPOSER)
            .highlight_with::<emoji::Highlighter>((), emoji::format)
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
            .padding(Padding::new(10.0).right(emoji::FIELD_INSET))
            .max_height(200);
        let composer = emoji::in_field(
            composer,
            self.composer_emoji.is_some(),
            false,
            Message::ComposerEmoji(emoji::Message::Open),
        );
        let panel = self.composer_emoji.as_ref().map(|panel| {
            panel.view(
                look.frequent_emoji,
                Message::InsertEmoji,
                Message::ComposerEmoji,
            )
        });
        let composer = popover(composer, panel, Side::Above, Edge::Right)
            .on_dismiss(Message::ComposerEmoji(emoji::Message::Close));
        let composer_empty = self.composer.text().trim().is_empty();
        let send = hint(
            button(text("Send").size(13))
                .padding([10, 14])
                .style(style::primary_button)
                .on_press_maybe((!composer_empty).then_some(Message::Send)),
            "Send now (Enter)",
            Position::Top,
        );
        // While open, a click on it closes the picker as any outside click.
        let picking = self.later.picker.is_some() && self.later.rescheduling.is_none();
        let later = hint(
            button(icons::clock(18.0, |theme| {
                theme
                    .extended_palette()
                    .background
                    .base
                    .text
                    .scale_alpha(0.75)
            }))
            .padding([9, 9])
            .style(style::secondary_button)
            .on_press_maybe((!picking).then_some(Message::Later(send_later::Message::Open))),
            "Send later: pick a time",
            Position::Top,
        );
        let picker = if self.later.rescheduling.is_none() {
            send_later::picker(&self.later, !composer_empty, scheduled.now)
        } else {
            None
        };
        let send = popover(
            row![send, later].spacing(4),
            picker,
            Side::Above,
            Edge::Right,
        )
        .on_dismiss(Message::Later(send_later::Message::Close));

        let mut bottom = column![]
            .spacing(6)
            .padding([8.0, TIMELINE_PADDING + MESSAGE_PADDING]);
        if !scheduled.messages.is_empty() {
            bottom = bottom.push(send_later::queue(
                scheduled,
                &self.later,
                self.editing.as_ref(),
                look.frequent_emoji,
            ));
        }
        if let Some(typing) = actions::typing(&self.typing) {
            bottom = bottom.push(typing);
        }
        if let Some(bar) = self.compose.bar() {
            bottom = bottom.push(bar);
        }
        let attach = hint(
            button(icons::paperclip(20.0, |theme| {
                theme
                    .extended_palette()
                    .background
                    .base
                    .text
                    .scale_alpha(0.75)
            }))
            .padding([8, 10])
            .style(style::secondary_button)
            .on_press(Message::Attach),
            "Attach files (or drop them on the window)",
            Position::Top,
        );
        bottom = bottom.push(
            row![composer, attach, send]
                .spacing(8)
                .align_y(Alignment::End),
        );
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
        // The name and picture open the room's info card.
        let info = summary.map_or(info::Target::Room(self.id.clone()), info::target_of);
        let about = mouse_area(
            row![avatar, title.width(Length::Fill)]
                .spacing(10)
                .align_y(Alignment::Center),
        )
        .interaction(mouse::Interaction::Pointer)
        .on_press(Message::ShowInfo(info));
        let about_tip = match summary.map(|s| s.kind) {
            Some(RoomKind::Direct) => "Who this is",
            _ => "Room settings and address",
        };
        let members_tip = if self.members.is_some() {
            "Hide the member list"
        } else {
            "Show who is in the room"
        };
        let mut header = column![
            row![
                hint(about, about_tip, Position::Bottom),
                self.calls.header_controls(look.call_services),
                hint(
                    button(text("Members").size(13))
                        .padding([5, 10])
                        .style(style::list_item(self.members.is_some()))
                        .on_press(Message::ToggleMembers),
                    members_tip,
                    Position::Bottom,
                ),
                hint(
                    button(icons::gear(16.0, |theme| {
                        theme
                            .extended_palette()
                            .background
                            .base
                            .text
                            .scale_alpha(0.7)
                    }))
                    .padding([5, 8])
                    .style(style::list_item(false))
                    .on_press(Message::ShowInfo(info::Target::Room(self.id.clone()))),
                    "Room settings: name, notifications, favourite",
                    Position::Bottom,
                ),
            ]
            .spacing(10)
            .align_y(Alignment::Center)
            .padding([8.0, TIMELINE_PADDING + MESSAGE_PADDING]),
        ];
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
    /// The user's most used emoji, for reactions.
    pub frequent_emoji: &'a [String],
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
    picker: Option<(&'b str, PickerAt)>,
    menu: Option<(&'b str, MenuAt)>,
    /// The selection, with each message's index in the timeline.
    selected: Option<(Bounds, &'b HashMap<&'b str, usize>)>,
    dragging: bool,
    emoji: Option<&'a emoji::Panel>,
    /// The user's most used emoji.
    frequent_emoji: &'b [String],
    confirm_delete: Option<&'b str>,
    /// The message being edited in place, if any.
    editing: Option<&'a InlineEdit>,
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
    // Where this message's reaction window is open, if it is.
    let picking = context
        .picker
        .filter(|(id, _)| *id == event.id)
        .map(|(_, at)| at);
    // Where this message's menu is open, if it is.
    let menu_at = context
        .menu
        .filter(|(id, _)| *id == event.id)
        .map(|(_, at)| at);
    // Its math is shown typeset, so it can be saved as a picture.
    let mut math_shown = false;
    let select = || {
        let id = event.id.clone();
        message::Select {
            range: context
                .selected
                .and_then(|(bounds, order)| bounds.in_message(*order.get(event.id.as_str())?)),
            dragging: context.dragging,
            on_gesture: Rc::new(move |gesture| {
                Message::Select(selection::Message::Gesture(id.clone(), gesture))
            }),
        }
    };
    let body: Element<'a, Message> = match &event.content {
        EventContent::Message(m) => {
            let content: Element<'a, Message> = match (m.kind, m.document.has_math()) {
                (MessageKind::Text | MessageKind::Notice | MessageKind::Emote, true) => {
                    match math.get(&Math::key(&m.document, params)) {
                        Some(Rendered::Image {
                            handle,
                            width,
                            height,
                            text,
                        }) => {
                            math_shown = true;
                            let select = select();
                            selectable_image(
                                image(handle.clone()).width(*width).height(*height),
                                text.clone(),
                            )
                            .selected(select.range)
                            .dragging(select.dragging)
                            .on_gesture(select.on_gesture)
                            .into()
                        }
                        _ => message::view(&m.document, theme, Message::LinkClicked, select()),
                    }
                }
                (MessageKind::Text | MessageKind::Notice | MessageKind::Emote, false) => {
                    message::view(&m.document, theme, Message::LinkClicked, select())
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
                let picker = (picking == Some(PickerAt::Reactions)).then(|| {
                    actions::picker(event, &m.reactions, context.frequent_emoji, context.emoji)
                });
                parts = parts.push(actions::reactions(event, &m.reactions, picker));
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

    let mut item = column![]
        .spacing(2)
        .width(Length::Fill)
        .padding(Padding::ZERO.right(TEXT_MARGIN));
    if with_header {
        item = item.push(
            mouse_area(sender_line(event, context.colored_names))
                .interaction(mouse::Interaction::Pointer)
                .on_press(sender_info(event)),
        );
    }
    let edit = context
        .editing
        .filter(|e| e.target == Editing::Sent(event.key.clone()));
    item = item.push(match edit {
        Some(edit) => edit.view(context.frequent_emoji),
        None => body,
    });
    let message = match &event.content {
        EventContent::Message(m) => Some(m),
        _ => None,
    };
    let in_selection = context.selected.is_some_and(|(bounds, order)| {
        order
            .get(event.id.as_str())
            .is_some_and(|&i| bounds.in_message(i).is_some())
    });
    let menu_items = message
        .map(|m| message_menu::items(m, math_shown, in_selection))
        .filter(|items| !items.is_empty());
    let menu_view = |at| {
        (menu_at == Some(at))
            .then(|| menu_items.clone().map(message_menu::view))
            .flatten()
    };
    if context.confirm_delete == Some(event.id.as_str()) {
        item = item.push(actions::confirm_delete(event));
    }
    if let Some(markers) = actions::read_markers(event, context.avatars) {
        item = item.push(markers);
    }
    match &event.send_state {
        SendState::Sent => {}
        SendState::Sending => item = item.push(text("sending…").size(11).style(text::secondary)),
        SendState::Waiting => {
            item = item.push(
                text("waiting for connection…")
                    .size(11)
                    .style(text::secondary),
            )
        }
        SendState::Failed(e) => {
            item = item.push(text(format!("not sent: {e}")).size(11).style(text::danger))
        }
    }

    // The avatar starts a group; follow-ups show their time there on hover.
    let hovered = context.hovered == Some(event.id.as_str());
    let gutter: Element<'a, Message> = if with_header {
        mouse_area(context.avatars.view(
            event.sender.as_str(),
            &display_name(event),
            event.sender_avatar.as_deref(),
            SENDER_AVATAR,
            Shape::Circle,
        ))
        .interaction(mouse::Interaction::Pointer)
        .on_press(sender_info(event))
        .into()
    } else if hovered {
        container(text(time(event)).size(10).style(text::secondary))
            .center_x(SENDER_AVATAR)
            .height(Length::Fill)
            .padding(Padding::ZERO.top(5))
            .into()
    } else {
        space().width(SENDER_AVATAR).into()
    };
    let line = if with_header {
        row![gutter, item].spacing(GUTTER_SPACING)
    } else {
        // At least as tall as the toolbar, with a short line (e.g. of math)
        // centred; the time stays at the top of longer messages.
        let strut = space().width(GUTTER_SPACING).height(TOOLBAR_HEIGHT);
        row![gutter, strut, item].align_y(Alignment::Center)
    };
    let top = if with_header { 4.0 } else { 1.0 };
    let row = container(line)
        .padding(Padding::new(MESSAGE_PADDING).top(top).bottom(1))
        .width(Length::Fill)
        .style(style::message_row(hovered));
    // Actions float over the top right of a hovered message.
    let row: Element<'a, Message> = match message {
        // Also while its reaction window is open.
        Some(m)
            if (hovered
                || picking == Some(PickerAt::Toolbar)
                || menu_at == Some(MenuAt::Toolbar))
                && event.event_id.is_some()
                && edit.is_none() =>
        {
            let preview = first_line(&m.document.to_plain_text());
            let target = Target::of(event, display_name(event), preview);
            let edit_text = Some(m.body.clone());
            let picker = (picking == Some(PickerAt::Toolbar)).then(|| {
                actions::picker(event, &m.reactions, context.frequent_emoji, context.emoji)
            });
            let menu = menu_items
                .is_some()
                .then(|| actions::Menu(menu_view(MenuAt::Toolbar)));
            stack![
                row,
                container(actions::toolbar(event, target, edit_text, picker, menu))
                    .align_right(Length::Fill)
                    .padding(Padding::ZERO.top(1.0).right(8.0)),
            ]
            .into()
        }
        _ => row.into(),
    };
    // Right-click opens the menu where the cursor is.
    let row: Element<'a, Message> = if menu_items.is_some() {
        context_menu(
            row,
            menu_view(MenuAt::Cursor),
            act(actions::Message::OpenMenu(event.id.clone(), MenuAt::Cursor)),
        )
        .on_dismiss(act(actions::Message::CloseMenu))
        .into()
    } else {
        row
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

/// Opens the sender's info card.
fn sender_info(event: &Event) -> Message {
    Message::ShowInfo(info::sender(
        event.sender.clone(),
        event.sender_name.clone(),
        event.sender_avatar.clone(),
    ))
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
