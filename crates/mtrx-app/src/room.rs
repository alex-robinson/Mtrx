//! An open room: timeline and composer.

use std::{
    collections::HashMap,
    rc::Rc,
    sync::Arc,
    time::{Duration, SystemTime},
};

use chrono::{DateTime, Local};
use iced::{
    Alignment, Element, Length, Padding, Rectangle, Task, Theme, Vector,
    advanced::widget::{Id, Operation, operation},
    keyboard::{Key, key::Named},
    mouse,
    widget::{
        button, column, container, image, mouse_area, row, rule, scrollable, scrollable::Viewport,
        space, stack, text, text_editor,
    },
};
use mtrx_core::{
    CallService, Event, EventContent, Media, Member, MembershipChange, MessageKind, MxcUri,
    OwnedEventId, OwnedRoomId, OwnedUserId, RoomKind, RoomSummary, RoomTimeline, ScheduledMessage,
    SendState, TimelineEntry, UserId,
};

use crate::{
    actions::{self, Compose, Editing, InlineEdit, PickerAt, Target, act},
    app::Message,
    avatar::{self, Avatars, Shape},
    calls::{self, RoomCalls},
    context_menu::context_menu,
    drag_scroll::drag_scroll,
    ellipsis::ellipsis,
    emoji,
    find::{self, Find},
    fonts,
    hint::{Position, hint},
    icons, info,
    math::{self, Math, Rendered},
    media::{self, Attached, Previews},
    members,
    mention::{self, Candidate, Completion},
    message,
    message_menu::{self, MenuAt},
    popover::{Edge, Side, popover},
    presence::{self, Presences},
    selectable,
    selectable_image::selectable_image,
    selection::{self, Bounds, Point, Pos, Selection, Step, units_text},
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
/// Right padding of the composer's text, clear of its paperclip and smiley.
const COMPOSER_INSET: f32 = emoji::FIELD_INSET + 29.0;
/// The timeline's scrollable.
pub const TIMELINE: &str = "timeline";

/// Consecutive messages of one sender within this gap share a header.
const GROUP_GAP: Duration = Duration::from_secs(5 * 60);
/// How long our message is being sent before its clock shows, so a fast
/// send never flashes one.
pub const SENDING_CLOCK: Duration = Duration::from_millis(300);

/// Left edge of message text within the timeline column.
const TEXT_INDENT: f32 = MESSAGE_PADDING + SENDER_AVATAR + GUTTER_SPACING;

pub struct Room {
    pub id: OwnedRoomId,
    pub timeline: Arc<RoomTimeline>,
    pub entries: Vec<TimelineEntry>,
    pub composer: text_editor::Content,
    /// Files sent after the composer's text.
    pub attachments: Vec<Attached>,
    pub later: SendLater,
    pub calls: RoomCalls,
    pub compose: Compose,
    /// A sent or scheduled message being edited in place.
    pub editing: Option<InlineEdit>,
    /// The timeline row whose reaction window is open, and where.
    pub picker: Option<(String, PickerAt)>,
    /// The timeline row whose menu is open, and where.
    pub menu: Option<(String, MenuAt)>,
    /// The timeline row whose readers are listed.
    pub readers: Option<String>,
    /// The timeline row and reaction key whose people are listed.
    pub reactors: Option<(String, String)>,
    /// The timeline row whose message was not sent and is offered to retry
    /// or delete.
    pub unsent: Option<String>,
    /// Our messages being sent.
    sending: Sending,
    /// Text selected in the timeline.
    pub selection: Option<Selection>,
    /// The find bar (⌘F), while open.
    pub find: Option<Find>,
    /// All emoji in the reaction picker, while shown.
    pub emoji: Option<emoji::Panel>,
    /// The composer's emoji panel, while shown.
    pub composer_emoji: Option<emoji::Panel>,
    /// The timeline row whose deletion awaits confirmation.
    pub confirm_delete: Option<String>,
    /// Names of others typing.
    pub typing: Vec<String>,
    /// The members once loaded, or why loading failed: for the member list
    /// and mentions.
    pub member_list: Option<Result<Vec<Member>, String>>,
    /// The members are being loaded.
    pub loading_members: bool,
    /// What `@` may mention, see [`mention::texts`].
    pub mentions: Arc<[String]>,
    /// We may mention everyone with `@room`.
    pub can_mention_room: bool,
    /// Completing a mention in the composer.
    pub completion: Option<Completion>,
    /// How many of the newest entries are rendered.
    shown: usize,
    pub paginating: bool,
    /// A new timeline is being loaded (⌘R).
    pub reloading: bool,
    pub error: Option<String>,
    /// [`Event::id`] of the message under the cursor.
    hovered: Option<String>,
    /// [`Event::id`] of the message just gone to, e.g. a reply's original.
    pub marked: Option<String>,
    /// The message being gone to while older events load, and how many
    /// pages were loaded for it.
    pub seeking: Option<(OwnedEventId, usize)>,
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
        // Messages being sent already when the room opens show their clock.
        let entries = timeline.current();
        Self {
            calls: RoomCalls::new(id.clone()),
            compose: Compose::New,
            editing: None,
            picker: None,
            menu: None,
            readers: None,
            reactors: None,
            unsent: None,
            sending: Sending::already(&entries),
            selection: None,
            find: None,
            emoji: None,
            composer_emoji: None,
            confirm_delete: None,
            typing: Vec::new(),
            member_list: None,
            loading_members: false,
            mentions: Default::default(),
            can_mention_room: false,
            completion: None,
            id,
            entries,
            timeline,
            composer: text_editor::Content::new(),
            attachments: Vec::new(),
            later: SendLater::default(),
            shown: WINDOW,
            paginating: false,
            reloading: false,
            error: None,
            hovered: None,
            marked: None,
            seeking: None,
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
            self.set_composer(text_editor::Content::with_text(&edit.text()));
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

    /// Replace what the composer holds.
    pub fn set_composer(&mut self, content: text_editor::Content) {
        self.composer = content;
        self.update_completion();
    }

    /// The composer's text or cursor changed: complete the mention there.
    pub fn update_completion(&mut self) {
        self.completion = Completion::of(&self.composer, self.completion.as_ref());
    }

    /// Who the mention being completed may be.
    pub fn candidates(&self, own: &UserId) -> Vec<Candidate<'_>> {
        match (&self.completion, &self.member_list) {
            (Some(completion), Some(Ok(members))) => mention::candidates(
                &completion.query.text,
                members,
                &self.mentions,
                own,
                self.can_mention_room,
            ),
            _ => Vec::new(),
        }
    }

    /// The members were loaded (with whether we may mention the room), or
    /// not.
    pub fn members_loaded(&mut self, result: Result<(Vec<Member>, bool), String>) {
        self.loading_members = false;
        self.member_list = Some(result.map(|(members, can_mention_room)| {
            self.mentions = mention::texts(&members);
            self.can_mention_room = can_mention_room;
            members
        }));
    }

    /// Complete the mention being written with `mention`.
    pub fn complete(&mut self, mention: &str) {
        if let Some(completion) = self.completion.take() {
            mention::insert(&mut self.composer, &completion.query, mention);
        }
        self.update_completion();
    }

    /// Show a freshly loaded timeline, keeping what is being written.
    pub fn reload(&mut self, timeline: Arc<RoomTimeline>) {
        self.entries = timeline.current();
        self.sending = Sending::already(&self.entries);
        self.timeline = timeline;
        self.shown = WINDOW;
        self.paginating = false;
        self.reloading = false;
        self.picker = None;
        self.readers = None;
        self.reactors = None;
        self.unsent = None;
        self.confirm_delete = None;
        self.error = None;
        self.find_again();
    }

    /// Search the entries again for the find bar's text, as they changed.
    pub fn find_again(&mut self) {
        if let Some(find) = &mut self.find {
            find.search(&self.entries);
        }
    }

    /// Render the message with the current match of the find bar, and
    /// scroll to the match.
    pub fn show_found(&mut self) -> Task<Message> {
        let Some(event) = self
            .find
            .as_ref()
            .and_then(Find::current)
            .map(|m| m.event.clone())
        else {
            return Task::none();
        };
        if let Some(index) = (self.entries.iter())
            .position(|e| matches!(e, TimelineEntry::Event(e) if e.id == event))
        {
            self.shown = self.shown.max(self.entries.len() - index);
        }
        scroll_to_found(&event)
    }

    /// Follow which of our messages are being sent, once the entries
    /// changed. Returns the rows of those that started: their clock shows
    /// once [`Room::sending_slow`] after [`SENDING_CLOCK`].
    pub fn sending_changed(&mut self) -> Vec<String> {
        self.sending.changed(&self.entries)
    }

    /// Our message in row `id` started sending [`SENDING_CLOCK`] ago: its
    /// clock shows if it still is.
    pub fn sending_slow(&mut self, id: &str) {
        self.sending.slow(id);
    }

    /// Our newest message (any row but a state change): sent, it implies
    /// the older ones were, and read markers sit on the last message each
    /// reader read, so only it says it was sent.
    fn newest_own(&self) -> Option<&str> {
        self.entries.iter().rev().find_map(|entry| match entry {
            TimelineEntry::Event(event) if event.own && !event.is_state() => {
                Some(event.id.as_str())
            }
            _ => None,
        })
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

    /// Render the message `event` if it is loaded; its [`Event::id`].
    pub fn reveal(&mut self, event: &OwnedEventId) -> Option<String> {
        let index = self.entries.iter().position(
            |entry| matches!(entry, TimelineEntry::Event(e) if e.event_id.as_ref() == Some(event)),
        )?;
        self.shown = self.shown.max(self.entries.len() - index);
        match &self.entries[index] {
            TimelineEntry::Event(e) => Some(e.id.clone()),
            _ => None,
        }
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
    /// `members_shown`: with the member list's.
    pub fn avatars<'a>(
        &'a self,
        summary: Option<&'a RoomSummary>,
        own: &UserId,
        members_shown: bool,
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
                let listed = self.readers.as_deref() == Some(event.id.as_str());
                let reactors = (self.reactors.as_ref())
                    .filter(|(id, _)| *id == event.id)
                    .map(|(_, key)| key.as_str());
                sender
                    .into_iter()
                    .chain(actions::reader_avatars(event, listed))
                    .chain(actions::reactor_avatars(event, reactors))
            });
        let listed = match &self.member_list {
            Some(Ok(list)) if members_shown => list.as_slice(),
            _ => &[],
        };
        let members = members::avatars(listed);
        let candidates = self
            .candidates(own)
            .into_iter()
            .filter_map(|candidate| match candidate {
                Candidate::Member { member, .. } => Some(member),
                Candidate::Room => None,
            });
        let completed = members::avatars_of(candidates);
        room.into_iter()
            .chain(senders)
            .chain(members)
            .chain(completed)
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

    /// Move the end of the selection by `step`, on into the message before
    /// or after.
    pub fn extend(&mut self, step: Step) {
        fn units(entry: &TimelineEntry) -> Option<(&str, Vec<selection::Unit>)> {
            match entry {
                TimelineEntry::Event(event) => match &event.content {
                    EventContent::Message(m) => {
                        Some((event.id.as_str(), message::text_map(&m.document).units))
                    }
                    _ => None,
                },
                _ => None,
            }
        }
        let Some(selection) = self.selection.as_mut() else {
            return;
        };
        let Some(at) = (self.entries.iter())
            .position(|e| matches!(e, TimelineEntry::Event(e) if e.id == selection.focus.event))
        else {
            return;
        };
        let Some((_, here)) = units(&self.entries[at]) else {
            return;
        };
        selection.focus = match selection::step(&here, selection.focus.pos, step) {
            Some(pos) => Point {
                pos,
                ..selection.focus.clone()
            },
            None => {
                let next = if step.forward() {
                    self.entries[at + 1..].iter().find_map(units)
                } else {
                    self.entries[..at].iter().rev().find_map(units)
                };
                let Some((event, units)) = next else {
                    return;
                };
                let pos = match units.last() {
                    Some(last) if !step.forward() => Pos {
                        unit: units.len() - 1,
                        offset: last.text.len(),
                    },
                    _ => Pos::default(),
                };
                Point {
                    event: event.to_owned(),
                    pos,
                }
            }
        };
    }

    /// Move the end of the selection a line up or down, by the layout of
    /// the text holding it.
    pub fn extend_line(&self, up: bool) -> Task<Message> {
        let Some(focus) = self.selection.as_ref().map(|s| s.focus.clone()) else {
            return Task::none();
        };
        selectable::line_step(focus.pos.offset, up).map(move |moved| {
            Message::Select(selection::Message::LineMoved(
                focus.clone(),
                up,
                moved.flatten(),
            ))
        })
    }

    /// The end of the selection moved from `from` a line up or down to
    /// `offset` in its unit, or past the unit (`None`): on into the one
    /// before or after.
    pub fn line_moved(&mut self, from: Point, up: bool, offset: Option<usize>) {
        let Some(selection) = self.selection.as_mut().filter(|s| s.focus == from) else {
            return;
        };
        match offset {
            Some(offset) => selection.focus.pos.offset = offset,
            None => {
                // From the unit's edge, a step goes on into the next one.
                selection.focus.pos.offset = if up { 0 } else { usize::MAX };
                self.extend(if up { Step::Back } else { Step::Forward });
            }
        }
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

    /// The first and last message on screen (partly, or wholly), for ⌘A.
    pub fn on_screen(&self) -> Task<Option<(String, String)>> {
        let ids: Vec<String> = self
            .visible()
            .iter()
            .filter_map(|entry| match entry {
                TimelineEntry::Event(event)
                    if matches!(event.content, EventContent::Message(_)) =>
                {
                    Some(event.id.clone())
                }
                _ => None,
            })
            .collect();
        let rows = ids.iter().map(|id| row_id(id)).collect();
        iced::advanced::widget::operate(FindRows {
            rows,
            view: None,
            found: Vec::new(),
        })
        .map(move |range| range.map(|(first, last)| (ids[first].clone(), ids[last].clone())))
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
            own_user: look.own_user,
            math: look.math,
            params: &look.params,
            avatars: look.avatars,
            previews: look.previews,
            colored_names: look.colored_names,
            call_services: look.call_services,
            hovered: self.hovered.as_deref(),
            marked: self.marked.as_deref(),
            picker: self.picker.as_ref().map(|(id, at)| (id.as_str(), *at)),
            menu: self.menu.as_ref().map(|(id, at)| (id.as_str(), at)),
            readers: self.readers.as_deref(),
            reactors: (self.reactors.as_ref()).map(|(id, key)| (id.as_str(), key.as_str())),
            unsent: self.unsent.as_deref(),
            newest_own: self.newest_own(),
            sending: &self.sending,
            selected,
            found: self.find.as_ref(),
            focus: self.selection.as_ref().map(|s| &s.focus),
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
                && event.is_state()
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
        let candidates = self.candidates(look.own_user);
        let completing = !candidates.is_empty();
        let selected = self.composer.selection().is_some();
        let composer = text_editor(&self.composer)
            .id(COMPOSER)
            .highlight_with::<mention::Highlighter>(self.mentions.clone(), mention::format)
            .style(style::editor)
            .placeholder("Message…  (Markdown, $…$ for math; Shift+Enter for a new line)")
            .on_action(Message::Composer)
            .key_binding(move |press| {
                let custom = |message| Some(text_editor::Binding::Custom(message));
                match press.key {
                    Key::Named(Named::ArrowUp) if completing => {
                        custom(Message::Mention(mention::Message::Up))
                    }
                    Key::Named(Named::ArrowDown) if completing => {
                        custom(Message::Mention(mention::Message::Down))
                    }
                    Key::Named(Named::Enter | Named::Tab) if completing => {
                        custom(Message::Mention(mention::Message::Choose))
                    }
                    // The window's Esc closes the completion.
                    Key::Named(Named::Escape) if completing => None,
                    Key::Named(Named::Enter) if !press.modifiers.shift() => custom(Message::Send),
                    Key::Named(Named::Escape) if cancellable => {
                        custom(Message::Act(actions::Message::CancelCompose))
                    }
                    _ => selection::editor_key(press, selected),
                }
            })
            .padding(
                Padding::new(10.0)
                    .right(COMPOSER_INSET)
                    .top(if self.attachments.is_empty() {
                        10.0
                    } else {
                        media::CHIP_HEIGHT + 18.0
                    }),
            )
            .max_height(200);
        let attach = hint(
            button(icons::paperclip(19.0, |theme| {
                theme
                    .extended_palette()
                    .background
                    .base
                    .text
                    .scale_alpha(0.55)
            }))
            .padding(4)
            .style(style::list_item(false))
            .on_press(Message::Attach),
            "Attach files (or drop them on the window)",
            Position::Top,
        );
        let toggle = emoji::toggle(
            self.composer_emoji.is_some(),
            false,
            Message::ComposerEmoji(emoji::Message::Open),
        );
        // The attached files sit inside the field, above the text.
        let mut composer = stack![
            composer,
            container(row![attach, toggle].spacing(2))
                .align_right(Length::Fill)
                .align_bottom(Length::Fill)
                .padding(Padding::new(6.0).top(0)),
        ];
        if !self.attachments.is_empty() {
            composer = composer.push(
                container(media::attached_view(&self.attachments, Message::Unattach))
                    .padding(Padding::new(8.0).right(COMPOSER_INSET)),
            );
        }
        let panel = self.composer_emoji.as_ref().map(|panel| {
            panel.view(
                look.frequent_emoji,
                Message::InsertEmoji,
                Message::ComposerEmoji,
            )
        });
        let composer = popover(composer, panel, Side::Above, Edge::Right)
            .on_dismiss(Message::ComposerEmoji(emoji::Message::Close))
            .click_through();
        let selected = self.completion.as_ref().map_or(0, |c| c.selected);
        let completion = completing.then(|| mention::view(&candidates, selected, look.avatars));
        let composer = popover(composer, completion, Side::Above, Edge::Left)
            .on_dismiss(Message::Mention(mention::Message::Close))
            .click_through();
        let composer_empty = self.composer.text().trim().is_empty() && self.attachments.is_empty();
        let send = hint(
            button(icons::send(18.0, |theme| {
                theme.extended_palette().primary.base.text
            }))
            .padding([9, 12])
            .style(style::primary_button)
            .on_press_maybe((!composer_empty).then_some(Message::Send)),
            "Send (Enter)",
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

        // Bars above and below the composer come and go inside columns that
        // are always there: widgets keep their state by position, so the
        // composer must stay where it is to keep its focus.
        let mut above = column![].spacing(6);
        if !scheduled.messages.is_empty() {
            above = above.push(send_later::queue(
                scheduled,
                &self.later,
                self.editing.as_ref(),
                look.frequent_emoji,
            ));
        }
        above = above.push(actions::typing(&self.typing));
        if let Some(bar) = self.compose.bar() {
            above = above.push(bar);
        }
        let mut below = column![];
        if let Some(error) = &self.error {
            below = below.push(text(error).size(12).style(text::danger));
        }
        let bottom = column![
            above,
            row![composer, send].spacing(8).align_y(Alignment::End),
            below,
        ]
        .spacing(6)
        .padding([8.0, TIMELINE_PADDING + MESSAGE_PADDING]);

        // Always there, so the timeline and composer keep their state.
        let mut find_bar = column![];
        if let Some(find) = &self.find {
            find_bar = find_bar.push(find.view(Message::Find));
            find_bar = find_bar.push(rule::horizontal(1));
        }
        let main = column![find_bar, timeline, bottom].width(Length::Fill);
        // The member list comes after, so it does not move the composer.
        let mut body = row![main];
        if look.show_members {
            let name = summary.map_or("", |s| s.name.as_str());
            body = body.push(members::view(
                self.member_list.as_ref(),
                &self.id,
                name,
                look.own_user,
                look.avatars,
                look.presences,
            ));
        }
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
        // A direct chat's person, as long as their presence is known.
        let presence = summary
            .filter(|s| s.kind == RoomKind::Direct && !s.is_notes(look.own_user))
            .and_then(|s| s.direct_user.as_ref())
            .and_then(|user| look.presences.get(user));
        let mut title = column![ellipsis(name).size(16).font(fonts::SEMIBOLD)];
        if let Some(line) = presence.and_then(|p| presence::line(p, look.now)) {
            title = title.push(ellipsis(line).size(12).style(text::secondary));
        }
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
        let avatar = presence::with_dot(avatar, presence, HEADER_AVATAR, |theme| {
            theme.extended_palette().background.base.color
        });
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
        let members_tip = if look.show_members {
            "Hide the member list"
        } else {
            "Show who is in the room"
        };
        column![
            row![
                hint(about, about_tip, Position::Bottom),
                self.calls.header_controls(look.call_services),
                hint(
                    button(text("Members").size(13))
                        .padding([5, 10])
                        .style(style::list_item(look.show_members))
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
            rule::horizontal(1).style(style::separator),
        ]
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
    /// The member list is shown beside the room.
    pub show_members: bool,
    /// People's presence, in the member list and a direct chat's header.
    pub presences: &'a Presences,
    /// For how long ago people were last active.
    pub now: SystemTime,
}

/// What rendering an event needs besides the event.
struct Context<'a, 'b> {
    theme: &'b Theme,
    /// Us, whose mentions stand out.
    own_user: &'b UserId,
    math: &'a Math,
    params: &'b math::Params,
    avatars: &'b Avatars,
    previews: &'b Previews,
    colored_names: bool,
    call_services: &'b [CallService],
    hovered: Option<&'b str>,
    marked: Option<&'b str>,
    picker: Option<(&'b str, PickerAt)>,
    menu: Option<(&'b str, &'b MenuAt)>,
    /// The row whose readers are listed.
    readers: Option<&'b str>,
    /// The row and reaction key whose people are listed.
    reactors: Option<(&'b str, &'b str)>,
    /// The row whose unsent message is offered to retry or delete.
    unsent: Option<&'b str>,
    /// Our newest message, the only one to say it was sent.
    newest_own: Option<&'b str>,
    /// Our messages being sent.
    sending: &'b Sending,
    /// The selection, with each message's index in the timeline.
    selected: Option<(Bounds, &'b HashMap<&'b str, usize>)>,
    /// The find bar, to highlight its matches.
    found: Option<&'b Find>,
    /// Where the selection ends.
    focus: Option<&'b Point>,
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
    // Shown as text, which carries the mark of an edit.
    let mut as_text = false;
    let select = || {
        let id = event.id.clone();
        message::Select {
            range: context
                .selected
                .and_then(|(bounds, order)| bounds.in_message(*order.get(event.id.as_str())?)),
            focus: context
                .focus
                .filter(|focus| focus.event == event.id)
                .map(|focus| focus.pos),
            dragging: context.dragging,
            on_gesture: Rc::new(move |gesture| {
                Message::Select(selection::Message::Gesture(id.clone(), gesture))
            }),
            found: (context.found).map_or_else(Vec::new, |find| find.in_message(&event.id)),
        }
    };
    let links = || {
        let id = event.id.clone();
        message::Links {
            open: Message::LinkClicked,
            menu: Rc::new(move |url| {
                act(actions::Message::OpenMenu(id.clone(), MenuAt::Link(url)))
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
                            let links = links();
                            selectable_image(
                                image(handle.clone()).width(*width).height(*height),
                                text.clone(),
                            )
                            .selected(select.range)
                            .found(select.found)
                            .dragging(select.dragging)
                            .on_link(links.open)
                            .on_link_menu(links.menu)
                            .on_gesture(select.on_gesture)
                            .into()
                        }
                        _ => {
                            as_text = true;
                            message::view(
                                &m.document,
                                theme,
                                context.own_user,
                                links(),
                                select(),
                                m.edited,
                            )
                        }
                    }
                }
                (MessageKind::Text | MessageKind::Notice | MessageKind::Emote, false) => {
                    as_text = true;
                    message::view(
                        &m.document,
                        theme,
                        context.own_user,
                        links(),
                        select(),
                        m.edited,
                    )
                }
                (kind, _) => match &m.media {
                    Some(file) => media::view(
                        file,
                        &m.body,
                        context.previews,
                        context.hovered == Some(event.id.as_str()),
                    ),
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
            if m.edited && !as_text {
                parts = parts.push(message::edited_line());
            }
            if !m.reactions.is_empty() {
                let picker = (picking == Some(PickerAt::Reactions)).then(|| {
                    actions::picker(event, &m.reactions, context.frequent_emoji, context.emoji)
                });
                let listed = (context.reactors)
                    .filter(|(id, _)| *id == event.id)
                    .map(|(_, key)| key);
                parts = parts.push(actions::reactions(
                    event,
                    &m.reactions,
                    listed,
                    context.avatars,
                    picker,
                ));
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
        | EventContent::CallLink { .. }
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
    let menu_view = |at| match menu_at {
        Some(MenuAt::Link(url)) if at == MenuAt::Cursor => {
            Some(message_menu::view(message_menu::link_items(url)))
        }
        Some(open) if *open == at => menu_items.clone().map(message_menu::view),
        _ => None,
    };
    if context.confirm_delete == Some(event.id.as_str()) {
        item = item.push(actions::confirm_delete(event));
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
        .style(style::message_row(
            hovered,
            context.marked == Some(event.id.as_str()),
        ));
    // Read markers, or while nobody read our message whether it is sent,
    // sit in the free margin at the bottom right, so neither ever changes
    // the row's height.
    let listed = context.readers == Some(event.id.as_str());
    let markers = actions::read_markers(event, context.avatars, listed).or_else(|| {
        actions::send_status(
            event,
            actions::Status {
                newest: context.newest_own == Some(event.id.as_str()),
                slow: context.sending.shows_clock(&event.id),
                open: context.unsent == Some(event.id.as_str()),
            },
        )
    });
    let markers = markers.map(|markers| {
        container(markers)
            .align_right(Length::Fill)
            .align_bottom(Length::Fill)
            .padding(Padding::ZERO.right(MESSAGE_PADDING).bottom(1.0))
            .into()
    });
    // Actions float over the top right of a hovered message, above the
    // markers of a short one.
    let toolbar: Option<Element<'a, Message>> = match message {
        // Also while its reaction window is open.
        Some(m)
            if (hovered
                || picking == Some(PickerAt::Toolbar)
                || menu_at == Some(&MenuAt::Toolbar))
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
            Some(
                container(actions::toolbar(event, target, edit_text, picker, menu))
                    .align_right(Length::Fill)
                    .padding(Padding::ZERO.top(1.0).right(8.0))
                    .into(),
            )
        }
        _ => None,
    };
    let row: Element<'a, Message> = if markers.is_none() && toolbar.is_none() {
        row.into()
    } else {
        stack![row]
            .extend(markers.into_iter().chain(toolbar))
            .into()
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
    // Found by `on_screen`.
    let row = container(row).id(row_id(&event.id));
    if with_header {
        // Space between groups, outside the hover background.
        column![space().height(6), row].into()
    } else {
        row.into()
    }
}

/// Our messages being sent, by row, and whether they show their clock yet:
/// only once sending took [`SENDING_CLOCK`], so a fast send shows none.
#[derive(Debug, Default)]
struct Sending(HashMap<String, bool>);

impl Sending {
    /// Those being sent in `entries` when a room opens: they show their
    /// clock at once.
    fn already(entries: &[TimelineEntry]) -> Self {
        Self(
            Self::rows(entries)
                .map(|id| (id.to_owned(), true))
                .collect(),
        )
    }

    /// Follow the `entries`. Returns the rows that started sending.
    fn changed(&mut self, entries: &[TimelineEntry]) -> Vec<String> {
        let now: Vec<&str> = Self::rows(entries).collect();
        self.0.retain(|id, _| now.contains(&id.as_str()));
        let started: Vec<String> = now
            .into_iter()
            .filter(|id| !self.0.contains_key(*id))
            .map(ToOwned::to_owned)
            .collect();
        for id in &started {
            self.0.insert(id.clone(), false);
        }
        started
    }

    /// Row `id` started sending [`SENDING_CLOCK`] ago.
    fn slow(&mut self, id: &str) {
        if let Some(shown) = self.0.get_mut(id) {
            *shown = true;
        }
    }

    fn shows_clock(&self, id: &str) -> bool {
        self.0.get(id) == Some(&true)
    }

    fn rows(entries: &[TimelineEntry]) -> impl Iterator<Item = &str> {
        entries.iter().filter_map(|entry| match entry {
            TimelineEntry::Event(event) if event.own && event.send_state == SendState::Sending => {
                Some(event.id.as_str())
            }
            _ => None,
        })
    }
}

/// The container around a message's row.
fn row_id(event: &str) -> Id {
    Id::from(format!("row:{event}"))
}

/// Scroll the timeline to show the row of message `id` in the middle.
pub fn scroll_to_row(id: &str) -> Task<Message> {
    iced::advanced::widget::operate(FindRow {
        row: row_id(id),
        view: None,
        found: None,
    })
    .then(|offset| match offset {
        Some(y) => iced::widget::operation::scroll_to(
            TIMELINE,
            scrollable::AbsoluteOffset {
                x: None,
                y: Some(y),
            },
        ),
        None => Task::none(),
    })
}

/// The offset of the timeline (from the bottom, where it is anchored)
/// with a row in the middle.
struct FindRow {
    row: Id,
    /// The timeline's bounds and content height.
    view: Option<(Rectangle, f32)>,
    found: Option<Rectangle>,
}

impl Operation<Option<f32>> for FindRow {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Option<f32>>)) {
        operate(self);
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        content_bounds: Rectangle,
        _translation: Vector,
        _state: &mut dyn operation::Scrollable,
    ) {
        if id == Some(&Id::new(TIMELINE)) {
            self.view = Some((bounds, content_bounds.height));
        }
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if id == Some(&self.row) {
            self.found = Some(bounds);
        }
    }

    fn finish(&self) -> operation::Outcome<Option<f32>> {
        operation::Outcome::Some(
            (self.view.zip(self.found)).map(|((view, content), row)| centred(view, content, row)),
        )
    }
}

/// The offset of a timeline (`view`, with `content` high) from the bottom,
/// where it is anchored, with `target` in the middle.
fn centred(view: Rectangle, content: f32, target: Rectangle) -> f32 {
    // From the top, then from the bottom.
    let top = target.y - view.y - (view.height - target.height).max(0.0) / 2.0;
    ((content - view.height).max(0.0) - top.max(0.0)).max(0.0)
}

/// Scroll the timeline to the current match of the find bar, in message
/// `event`, unless it is in view: to the middle, as browsers do.
fn scroll_to_found(event: &str) -> Task<Message> {
    iced::advanced::widget::operate(FindFound {
        row: row_id(event),
        view: None,
        found: None,
        row_found: None,
    })
    .then(|offset| match offset {
        Some(y) => iced::widget::operation::scroll_to(
            TIMELINE,
            scrollable::AbsoluteOffset {
                x: None,
                y: Some(y),
            },
        ),
        None => Task::none(),
    })
}

/// Where the timeline should be scrolled to show the current match, if it
/// is not in view.
struct FindFound {
    /// The row of its message, for when its text does not report it (e.g.
    /// while being edited).
    row: Id,
    /// The timeline's bounds, content height and how far it is scrolled.
    view: Option<(Rectangle, f32, Vector)>,
    found: Option<Rectangle>,
    row_found: Option<Rectangle>,
}

impl Operation<Option<f32>> for FindFound {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Option<f32>>)) {
        operate(self);
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        content_bounds: Rectangle,
        translation: Vector,
        _state: &mut dyn operation::Scrollable,
    ) {
        if id == Some(&Id::new(TIMELINE)) {
            self.view = Some((bounds, content_bounds.height, translation));
        }
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if id == Some(&self.row) {
            self.row_found = Some(bounds);
        }
    }

    fn custom(&mut self, id: Option<&Id>, bounds: Rectangle, _state: &mut dyn std::any::Any) {
        if id == Some(&Id::new(find::CURRENT)) {
            self.found = Some(bounds);
        }
    }

    fn finish(&self) -> operation::Outcome<Option<f32>> {
        let target = self.found.or(self.row_found);
        operation::Outcome::Some(self.view.zip(target).and_then(
            |((view, content, scrolled), target)| {
                let top = view.y + scrolled.y;
                let shown = target.y >= top && target.y + target.height <= top + view.height;
                (!shown).then(|| centred(view, content, target))
            },
        ))
    }
}

/// Where the timeline is and which messages' rows are in its view.
struct FindRows {
    rows: Vec<Id>,
    /// The timeline's bounds and how far it is scrolled.
    view: Option<(Rectangle, Vector)>,
    /// Each found row: its index in `rows` and bounds.
    found: Vec<(usize, Rectangle)>,
}

impl Operation<Option<(usize, usize)>> for FindRows {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Option<(usize, usize)>>)) {
        operate(self);
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        _content_bounds: Rectangle,
        translation: Vector,
        _state: &mut dyn operation::Scrollable,
    ) {
        if id == Some(&Id::new(TIMELINE)) {
            self.view = Some((bounds, translation));
        }
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if let Some(index) = id.and_then(|id| self.rows.iter().position(|row| row == id)) {
            self.found.push((index, bounds));
        }
    }

    fn finish(&self) -> operation::Outcome<Option<(usize, usize)>> {
        operation::Outcome::Some(
            self.view
                .and_then(|(view, scrolled)| on_screen(view, scrolled.y, &self.found)),
        )
    }
}

/// The first and last of `rows` (indices and unscrolled bounds) in the
/// view of a timeline scrolled by `scrolled`.
fn on_screen(
    view: Rectangle,
    scrolled: f32,
    rows: &[(usize, Rectangle)],
) -> Option<(usize, usize)> {
    let (top, bottom) = (view.y + scrolled, view.y + scrolled + view.height);
    let shown = rows
        .iter()
        .filter(|(_, row)| row.y < bottom && row.y + row.height > top)
        .map(|(index, _)| *index);
    Some((shown.clone().min()?, shown.max()?))
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
            EventContent::CallLink { url: Some(url) } => {
                format!("{name} changed the call link to {url}")
            }
            EventContent::CallLink { url: None } => format!("{name} removed the call link"),
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

#[cfg(test)]
mod tests {
    use iced::{Point, Size};

    use super::*;

    #[test]
    fn summarises_call_link_changes() {
        let alice: mtrx_core::OwnedUserId = "@alice:x".try_into().unwrap();
        let change = |id: &str, url: Option<&str>| Event {
            content: EventContent::CallLink {
                url: url.map(Into::into),
            },
            ..Event::test_message(id, alice.clone(), "")
        };
        let set = change("$a", Some("https://meet.jit.si/ice"));
        let removed = change("$b", None);
        assert_eq!(
            state_summary(&[&set, &removed]),
            "alice changed the call link to https://meet.jit.si/ice · alice removed the call link"
        );
    }

    #[test]
    fn finds_the_messages_on_screen() {
        // A timeline at y 100, 300 high, scrolled 500 down; rows 100 high.
        let view = Rectangle::new(Point::new(0.0, 100.0), Size::new(800.0, 300.0));
        let rows: Vec<_> = (0..12)
            .map(|i| {
                let row = Rectangle::new(
                    Point::new(0.0, 100.0 + 100.0 * i as f32),
                    Size::new(800.0, 100.0),
                );
                (i, row)
            })
            .collect();
        // Content 500..800 shows rows 5, 6 and 7.
        assert_eq!(on_screen(view, 500.0, &rows), Some((5, 7)));
        // Partly shown rows count.
        assert_eq!(on_screen(view, 450.0, &rows), Some((4, 7)));
        assert_eq!(on_screen(view, 5000.0, &rows), None);
    }

    #[test]
    fn scrolls_to_a_match_out_of_view() {
        // A timeline at y 100, 300 high, scrolled 500 down, 2000 of content.
        let view = Rectangle::new(Point::new(0.0, 100.0), Size::new(800.0, 300.0));
        let offset = |y| {
            let find = FindFound {
                row: row_id("$a"),
                view: Some((view, 2000.0, Vector::new(0.0, 500.0))),
                found: Some(Rectangle::new(Point::new(40.0, y), Size::new(30.0, 20.0))),
                row_found: None,
            };
            match find.finish() {
                operation::Outcome::Some(offset) => offset,
                _ => unreachable!(),
            }
        };
        // In view: stays.
        assert_eq!(offset(700.0), None);
        // Above: in the middle, counted from the bottom.
        assert_eq!(offset(300.0), Some(2000.0 - 300.0 - (200.0 - 140.0)));
        // Partly below.
        assert!(offset(890.0).is_some());
    }

    fn own_message(id: &str, send_state: SendState) -> TimelineEntry {
        let sender = "@alice:example.org".try_into().unwrap();
        TimelineEntry::Event(Event {
            own: true,
            send_state,
            ..Event::test_message(id, sender, "hi")
        })
    }

    #[test]
    fn the_sending_clock_shows_after_a_while() {
        let mut sending = Sending::default();
        // A send starts: no clock until it took a while.
        let entries = [own_message("a", SendState::Sending)];
        assert_eq!(sending.changed(&entries), ["a"]);
        assert!(!sending.shows_clock("a"));
        assert!(sending.changed(&entries).is_empty(), "started once");
        sending.slow("a");
        assert!(sending.shows_clock("a"));
        // Sent: gone; a late timer changes nothing.
        assert!(
            sending
                .changed(&[own_message("a", SendState::Sent)])
                .is_empty()
        );
        sending.slow("a");
        assert!(!sending.shows_clock("a"));
        // Retried after failing: waits again.
        assert_eq!(sending.changed(&entries), ["a"]);
        assert!(!sending.shows_clock("a"));
        // Being sent when the room opens: at once.
        assert!(Sending::already(&entries).shows_clock("a"));
    }
}
