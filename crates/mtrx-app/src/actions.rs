//! Actions on messages (react, reply, edit, delete, its menu), what the
//! composer is writing (a new message or a reply), and editing a message in
//! place.

use iced::{
    Alignment, Element, Length, Theme,
    keyboard::{Key, key::Named},
    widget::{button, column, container, row, space, text, text_editor},
};
use mtrx_core::{Event, EventKey, MxcUri, OwnedEventId, Reaction, Reader, Reply};
use mtrx_format::Document;

use crate::{
    app,
    avatar::{Avatars, Shape},
    ellipsis::ellipsis,
    emoji, fonts,
    hint::{Position, hint},
    icons,
    message_menu::MenuAt,
    popover::{Edge, Side, popover},
    style,
};

/// Read markers shown per message before "+N".
const MAX_READERS: usize = 3;
const READER_AVATAR: f32 = 16.0;

#[derive(Debug, Clone)]
pub enum Message {
    React(EventKey, String),
    /// Open or close the reaction window of a timeline row, there.
    TogglePicker(String, PickerAt),
    ClosePicker,
    Reply(Target),
    /// Edit one of our sent messages where it is shown, starting from its text.
    Edit(EventKey, String),
    EditAction(text_editor::Action),
    SaveEdit,
    CancelEdit,
    EditSaved(Result<(), String>),
    /// All emoji in the reaction picker.
    Emoji(emoji::Message),
    /// The emoji panel of the message being edited.
    EditEmoji(emoji::Message),
    /// Put an emoji into the message being edited.
    EditInsertEmoji(String),
    /// Ask before deleting a timeline row's message.
    AskDelete(String),
    Delete(EventKey),
    CancelDelete,
    /// Open a timeline row's menu, there.
    OpenMenu(String, MenuAt),
    CloseMenu,
    /// Put text on the clipboard.
    Copy(String),
    /// Put the text selected in the timeline on the clipboard.
    CopySelection,
    /// Put a message with math on the clipboard as a picture.
    CopyImage(Document),
    /// Save a message with math as a picture.
    SaveImage(Document),
    /// Show a timeline row's text in place of it, to select from.
    SelectText(String, String),
    SelectAction(text_editor::Action),
    CloseSelect,
    /// Stop replying.
    CancelCompose,
    Done(Result<(), String>),
}

/// Where a message's reaction window floats from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerAt {
    /// The smiley of the hover toolbar.
    Toolbar,
    /// The smiley after the message's reactions.
    Reactions,
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
}

impl Compose {
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
                    hint(
                        button(text("×").size(14))
                            .padding([0, 6])
                            .style(style::text_button)
                            .on_press(act(Message::CancelCompose)),
                        "Stop replying (Esc)",
                        Position::Left,
                    ),
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

/// The field editing a message in place.
const EDITOR: &str = "inline-edit";

/// A message being edited where it is shown: a sent one in the timeline, or
/// a scheduled one in its row. The composer keeps its draft meanwhile.
pub struct InlineEdit {
    pub target: Editing,
    content: text_editor::Content,
    /// Its emoji panel, while shown.
    pub emoji: Option<emoji::Panel>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Editing {
    Sent(EventKey),
    /// By [`mtrx_core::ScheduledMessage::id`].
    Scheduled(String),
}

impl InlineEdit {
    pub fn new(target: Editing, text: &str) -> Self {
        let mut content = text_editor::Content::with_text(text);
        content.perform(text_editor::Action::Move(text_editor::Motion::DocumentEnd));
        Self {
            target,
            content,
            emoji: None,
        }
    }

    pub fn insert_emoji(&mut self, emoji: &str) {
        emoji::insert(&mut self.content, emoji);
    }

    /// Put the cursor into the editor.
    pub fn focus<T>() -> iced::Task<T> {
        iced::widget::operation::focus(EDITOR)
    }

    pub fn perform(&mut self, action: text_editor::Action) {
        self.content.perform(action);
    }

    pub fn text(&self) -> String {
        self.content.text().trim_end().to_owned()
    }

    /// The editor, with emoji, Save and Cancel; Enter saves, Esc cancels.
    pub fn view<'a>(&'a self, frequent: &[String]) -> Element<'a, app::Message> {
        let editor = text_editor(&self.content)
            .id(EDITOR)
            .highlight_with::<emoji::Highlighter>((), emoji::format)
            .style(style::editor)
            .on_action(|action| act(Message::EditAction(action)))
            .key_binding(|press| match press.key {
                Key::Named(Named::Enter) if !press.modifiers.shift() => {
                    Some(text_editor::Binding::Custom(act(Message::SaveEdit)))
                }
                _ => text_editor::Binding::from_key_press(press),
            })
            .padding(iced::Padding::new(6.0).right(emoji::FIELD_INSET_COMPACT))
            .max_height(240);
        let editor = emoji::in_field(
            editor,
            self.emoji.is_some(),
            true,
            act(Message::EditEmoji(emoji::Message::Open)),
        );
        let panel = self.emoji.as_ref().map(|panel| {
            panel.view(
                frequent,
                |emoji| act(Message::EditInsertEmoji(emoji)),
                |message| act(Message::EditEmoji(message)),
            )
        });
        let editor = popover(editor, panel, Side::Below, Edge::Right)
            .on_dismiss(act(Message::EditEmoji(emoji::Message::Close)));
        let empty = self.content.text().trim().is_empty();
        let view = column![
            editor,
            row![
                text("Enter saves · Esc cancels")
                    .size(11)
                    .style(text::secondary),
                space::horizontal(),
                button(text("Cancel").size(12))
                    .padding([2, 8])
                    .style(style::text_button)
                    .on_press(act(Message::CancelEdit)),
                button(text("Save").size(12))
                    .padding([2, 10])
                    .style(style::primary_button)
                    .on_press_maybe((!empty).then(|| act(Message::SaveEdit))),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        ]
        .spacing(4);
        view.into()
    }
}

fn tool_color(theme: &Theme) -> iced::Color {
    theme
        .extended_palette()
        .background
        .base
        .text
        .scale_alpha(0.7)
}

/// The toolbar's "⋯" for a message with a menu: the menu while open there.
pub struct Menu<'a>(pub Option<Element<'a, app::Message>>);

/// Symbols floating over a hovered message, each named on hover; the
/// reaction window floats from the smiley while `picker` is given.
pub fn toolbar<'a>(
    event: &Event,
    target: Option<Target>,
    edit_text: Option<String>,
    picker: Option<Element<'a, app::Message>>,
    menu: Option<Menu<'a>>,
) -> Element<'a, app::Message> {
    let tool = |icon: Element<'a, app::Message>, tip: &'static str, message: Option<Message>| {
        hint(
            button(icon)
                .padding([style::TOOL_PADDING, 6.0])
                .style(style::text_button)
                .on_press_maybe(message.map(act)),
            tip,
            Position::Top,
        )
    };
    // While open, a click on it closes the window as any outside click.
    let react = tool(
        icons::smiley(style::TOOL_ICON, tool_color),
        "React",
        picker
            .is_none()
            .then(|| Message::TogglePicker(event.id.clone(), PickerAt::Toolbar)),
    );
    let react =
        popover(react, picker, Side::Below, Edge::Right).on_dismiss(act(Message::ClosePicker));
    let mut bar = row![
        react,
        tool(
            icons::reply(style::TOOL_ICON, tool_color),
            "Reply",
            target.clone().map(Message::Reply)
        ),
    ]
    .spacing(2);
    if event.editable {
        bar = bar.push(tool(
            icons::pencil(style::TOOL_ICON, tool_color),
            "Edit",
            edit_text.map(|text| Message::Edit(event.key.clone(), text)),
        ));
    }
    if event.own && event.event_id.is_some() {
        bar = bar.push(tool(
            icons::trash(style::TOOL_ICON, tool_color),
            "Delete",
            Some(Message::AskDelete(event.id.clone())),
        ));
    }
    if let Some(Menu(open)) = menu {
        // While open, a click on it closes the menu as any outside click.
        let more = tool(
            text("⋯")
                .size(style::TOOL_ICON)
                .line_height(1.0)
                .style(|theme: &Theme| text::Style {
                    color: Some(tool_color(theme)),
                })
                .into(),
            "More: copy, select text",
            open.is_none()
                .then(|| Message::OpenMenu(event.id.clone(), MenuAt::Toolbar)),
        );
        bar = bar.push(
            popover(more, open, Side::Below, Edge::Right).on_dismiss(act(Message::CloseMenu)),
        );
    }
    container(bar)
        .padding(style::TOOLBAR_PADDING)
        .style(style::toolbar)
        .into()
}

/// The reaction window: the most used emoji, and below them all emoji
/// once "+" is clicked.
pub fn picker<'a>(
    event: &Event,
    reactions: &[Reaction],
    frequent: &[String],
    all: Option<&'a emoji::Panel>,
) -> Element<'a, app::Message> {
    let key = event.key.clone();
    let quick: Vec<Element<'a, app::Message>> = emoji::quick(frequent)
        .into_iter()
        .map(|emoji| {
            let own = reactions.iter().any(|r| r.key == emoji && r.own);
            button(text(emoji.to_owned()).size(18).font(fonts::EMOJI))
                .padding([2, 6])
                .style(style::list_item(own))
                .on_press(act(Message::React(key.clone(), emoji.to_owned())))
                .into()
        })
        .collect();
    let more = hint(
        button(text(if all.is_some() { "−" } else { "+" }).size(18))
            .padding([2, 8])
            .style(style::list_item(all.is_some()))
            .on_press(act(Message::Emoji(if all.is_some() {
                emoji::Message::Close
            } else {
                emoji::Message::Open
            }))),
        if all.is_some() {
            "Fewer emoji"
        } else {
            "All emoji"
        },
        Position::Top,
    );
    let mut window =
        column![row(quick).push(more).spacing(2).align_y(Alignment::Center)].spacing(4);
    if let Some(panel) = all {
        window = window.push(panel.view(
            &[],
            move |emoji| act(Message::React(key.clone(), emoji)),
            |message| act(Message::Emoji(message)),
        ));
    }
    container(window).padding(4).style(style::toolbar).into()
}

/// A message's reactions; clicking one adds or takes back ours. A smiley
/// after them opens the reaction window there (`picker`, while given).
pub fn reactions<'a>(
    event: &Event,
    reactions: &[Reaction],
    picker: Option<Element<'a, app::Message>>,
) -> Element<'a, app::Message> {
    let pills = reactions.iter().map(|reaction| {
        button(
            row![
                text(reaction.key.clone()).size(12).font(fonts::EMOJI),
                text(reaction.count.to_string()).size(12),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        )
        .padding([1, 7])
        .style(style::reaction(reaction.own))
        .on_press(act(Message::React(event.key.clone(), reaction.key.clone())))
        .into()
    });
    // While open, a click on it closes the window as any outside click.
    let open = picker.is_none();
    let react = hint(
        button(icons::smiley(14.0, tool_color))
            .padding([2, 6])
            .style(style::reaction(false))
            .on_press_maybe(
                open.then(|| act(Message::TogglePicker(event.id.clone(), PickerAt::Reactions))),
            ),
        "React",
        Position::Top,
    );
    let react =
        popover(react, picker, Side::Below, Edge::Left).on_dismiss(act(Message::ClosePicker));
    row(pills)
        .push(react)
        .spacing(4)
        .align_y(Alignment::Center)
        .wrap()
        .into()
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
    let mut markers = row![].spacing(2).align_y(Alignment::Center);
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
    // The names show over the pictures, which sit at the right.
    let markers = hint(
        markers,
        format!("Read by {}", names.join(", ")),
        Position::Top,
    );
    Some(container(markers).align_right(Length::Fill).into())
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

pub fn act(message: Message) -> app::Message {
    app::Message::Act(message)
}
