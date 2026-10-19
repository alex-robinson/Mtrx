//! Actions on messages (react, reply, edit, delete) and what the composer is
//! doing: a new message, a reply, or an edit.

use iced::{
    Alignment, Element, Length,
    widget::{button, container, row, space, text, tooltip},
};
use mtrx_core::{Event, EventKey, MxcUri, OwnedEventId, Reaction, Reader, Reply, ScheduledMessage};

use crate::{
    app,
    avatar::{Avatars, Shape},
    ellipsis::ellipsis,
    fonts, style,
};

/// Offered in the reaction picker.
const QUICK_REACTIONS: [&str; 8] = ["👍", "❤️", "😂", "🎉", "😮", "😢", "🙏", "👀"];
/// Read markers shown per message before "+N".
const MAX_READERS: usize = 3;
const READER_AVATAR: f32 = 16.0;

#[derive(Debug, Clone)]
pub enum Message {
    React(EventKey, String),
    /// Open or close the reaction picker of a timeline row.
    TogglePicker(String),
    Reply(Target),
    /// Load one of our messages into the composer to edit it.
    Edit(Target, String),
    /// Ask before deleting a timeline row's message.
    AskDelete(String),
    Delete(EventKey),
    CancelDelete,
    /// Back to writing a new message.
    CancelCompose,
    Done(Result<(), String>),
}

/// A message being replied to or edited.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub key: EventKey,
    pub event_id: OwnedEventId,
    pub sender: String,
    pub preview: String,
}

/// What the composer's text becomes when sent.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Compose {
    #[default]
    New,
    Reply(Target),
    Edit(Target),
    EditScheduled(ScheduledMessage),
}

impl Compose {
    pub fn scheduled(&self) -> Option<&ScheduledMessage> {
        match self {
            Self::EditScheduled(message) => Some(message),
            _ => None,
        }
    }

    pub fn reply_to(&self) -> Option<&OwnedEventId> {
        match self {
            Self::Reply(target) => Some(&target.event_id),
            _ => None,
        }
    }

    /// Esc leaves the mode.
    pub fn cancellable(&self) -> bool {
        *self != Self::New
    }

    /// The bar above the composer saying what is being written.
    pub fn bar<'a>(&self) -> Option<Element<'a, app::Message>> {
        let (label, preview) = match self {
            Self::New => return None,
            Self::Reply(target) => (
                format!("Replying to {}", target.sender),
                target.preview.clone(),
            ),
            Self::Edit(_) => (
                "Editing your message".into(),
                "Enter saves, Esc cancels".into(),
            ),
            Self::EditScheduled(_) => (
                "Editing a scheduled message".into(),
                "Enter saves, Esc cancels".into(),
            ),
        };
        Some(
            container(
                row![
                    text(label).size(12).font(fonts::SEMIBOLD),
                    text(preview)
                        .size(12)
                        .style(text::secondary)
                        .wrapping(text::Wrapping::None)
                        .width(Length::Fill),
                    button(text("×").size(14))
                        .padding([0, 6])
                        .style(style::text_button)
                        .on_press(act(Message::CancelCompose)),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([4, 10])
            .clip(true)
            .width(Length::Fill)
            .style(style::reply_quote)
            .into(),
        )
    }
}

impl Target {
    pub fn of(event: &Event, sender: String, preview: String) -> Option<Self> {
        Some(Self {
            key: event.key.clone(),
            event_id: event.event_id.clone()?,
            sender,
            preview,
        })
    }
}

/// Buttons floating over a hovered message.
pub fn toolbar<'a>(
    event: &Event,
    target: Option<Target>,
    edit_text: Option<String>,
) -> Element<'a, app::Message> {
    let tool = |label, message: Option<Message>| {
        button(text(label).size(12))
            .padding([2, 8])
            .style(style::text_button)
            .on_press_maybe(message.map(act))
    };
    let mut bar = row![
        tool("React", Some(Message::TogglePicker(event.id.clone()))),
        tool("Reply", target.clone().map(Message::Reply)),
    ]
    .spacing(2);
    if event.editable {
        bar = bar.push(tool(
            "Edit",
            target
                .clone()
                .zip(edit_text)
                .map(|(t, text)| Message::Edit(t, text)),
        ));
    }
    if event.own && event.event_id.is_some() {
        bar = bar.push(tool("Delete", Some(Message::AskDelete(event.id.clone()))));
    }
    container(bar).padding(2).style(style::toolbar).into()
}

/// The quick reactions for a message.
pub fn picker<'a>(event: &Event, reactions: &[Reaction]) -> Element<'a, app::Message> {
    let emojis = QUICK_REACTIONS.iter().map(|emoji| {
        let own = reactions.iter().any(|r| r.key == *emoji && r.own);
        button(text(*emoji).size(18))
            .padding([2, 6])
            .style(style::list_item(own))
            .on_press(act(Message::React(event.key.clone(), (*emoji).to_owned())))
            .into()
    });
    container(row(emojis).spacing(2))
        .padding(4)
        .style(style::toolbar)
        .into()
}

/// Reaction pills; clicking one adds or removes ours.
pub fn reactions<'a>(event: &Event, reactions: &[Reaction]) -> Element<'a, app::Message> {
    let pills = reactions.iter().map(|reaction| {
        button(text(format!("{} {}", reaction.key, reaction.count)).size(12))
            .padding([1, 7])
            .style(style::reaction(reaction.own))
            .on_press(act(Message::React(event.key.clone(), reaction.key.clone())))
            .into()
    });
    row(pills).spacing(4).wrap().into()
}

/// The start of the message a reply answers.
pub fn quote<'a>(reply: &'a Reply) -> Element<'a, app::Message> {
    let sender = reply
        .sender_name
        .as_deref()
        .or(reply.sender.as_ref().map(|s| s.localpart()))
        .unwrap_or("…");
    let preview = reply.preview.as_deref().unwrap_or("Loading…");
    row![
        container(space())
            .width(3)
            .height(18)
            .style(style::quote_accent),
        row![
            text(sender).size(12).font(fonts::SEMIBOLD),
            ellipsis(preview)
                .size(12)
                .style(text::secondary)
                .width(Length::Fill),
        ]
        .spacing(6),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

/// Asks before deleting.
pub fn confirm_delete<'a>(event: &Event) -> Element<'a, app::Message> {
    row![
        text("Delete this message for everyone?").size(12),
        button(text("Delete").size(12))
            .padding([2, 8])
            .style(style::danger_button)
            .on_press(act(Message::Delete(event.key.clone()))),
        button(text("Cancel").size(12))
            .padding([2, 8])
            .style(style::text_button)
            .on_press(act(Message::CancelDelete)),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

/// Small pictures of who read up to this message.
pub fn read_markers<'a>(event: &Event, avatars: &Avatars) -> Option<Element<'a, app::Message>> {
    if event.read_by.is_empty() {
        return None;
    }
    let mut markers = row![space::horizontal()]
        .spacing(2)
        .align_y(Alignment::Center);
    for reader in event.read_by.iter().take(MAX_READERS) {
        markers = markers.push(avatars.view(
            reader.id.as_str(),
            reader_name(reader),
            reader.avatar.as_deref(),
            READER_AVATAR,
            Shape::Circle,
        ));
    }
    if event.read_by.len() > MAX_READERS {
        markers = markers.push(
            text(format!("+{}", event.read_by.len() - MAX_READERS))
                .size(10)
                .style(text::secondary),
        );
    }
    let names: Vec<_> = event.read_by.iter().map(reader_name).collect();
    Some(
        tooltip(
            markers,
            container(text(format!("Read by {}", names.join(", "))).size(12))
                .padding(6)
                .style(container::rounded_box),
            tooltip::Position::Left,
        )
        .into(),
    )
}

fn reader_name(reader: &Reader) -> &str {
    reader.name.as_deref().unwrap_or(reader.id.localpart())
}

/// Pictures of readers shown, with their sizes.
pub fn reader_avatars(event: &Event) -> impl Iterator<Item = (&MxcUri, f32)> {
    event
        .read_by
        .iter()
        .take(MAX_READERS)
        .filter_map(|r| Some((r.avatar.as_deref()?, READER_AVATAR)))
}

/// Others typing, above the composer.
pub fn typing<'a>(names: &[String]) -> Option<Element<'a, app::Message>> {
    let line = match names {
        [] => return None,
        [one] => format!("{one} is typing…"),
        [one, two] => format!("{one} and {two} are typing…"),
        [one, rest @ ..] => format!("{one} and {} others are typing…", rest.len()),
    };
    Some(text(line).size(12).style(text::secondary).into())
}

fn act(message: Message) -> app::Message {
    app::Message::Act(message)
}
