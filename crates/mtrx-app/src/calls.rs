//! Calls through links: call cards in the timeline, the room header's call
//! controls (join the room's call link, start a call, set the link).

use iced::{
    Alignment, Element, Length, Theme,
    widget::{button, column, container, row, space, text, text_input},
};
use mtrx_core::{CallService, OwnedRoomId, ServiceKind, Url, recognize_call_link};

use crate::{
    app,
    ellipsis::ellipsis,
    fonts,
    hint::{Position, hint},
    icons, style,
};

#[derive(Debug, Clone)]
pub enum Message {
    /// The room's call link changed.
    Link(Option<Url>),
    /// Whether we may change the room's call link; `None` if unknown.
    CanSetLink(Option<bool>),
    /// The call service of the room's nearest space with one.
    SpaceService(Option<CallService>),
    ToggleMenu,
    Start(CallService),
    Started(Result<(), String>),
    EditLink,
    LinkInput(String),
    /// Fill the link editor with a new link of this service.
    NewLink(CallService),
    SaveLink,
    RemoveLink,
    CancelEdit,
    LinkSaved(Result<(), String>),
}

/// Call state of the open room.
pub struct RoomCalls {
    room: OwnedRoomId,
    pub link: Option<Url>,
    pub can_set_link: Option<bool>,
    pub space_service: Option<CallService>,
    pub menu: bool,
    /// The call link being edited.
    pub editor: Option<String>,
    /// A call is being started or the link saved.
    pub busy: bool,
    pub error: Option<String>,
}

impl RoomCalls {
    pub fn new(room: OwnedRoomId) -> Self {
        Self {
            room,
            link: None,
            can_set_link: None,
            space_service: None,
            menu: false,
            editor: None,
            busy: false,
            error: None,
        }
    }

    fn message(&self, message: Message) -> app::Message {
        app::Message::Calls(self.room.clone(), message)
    }

    /// The service "Start call" uses: the space's, else the first own one.
    pub fn default_service<'a>(&'a self, own: &'a [CallService]) -> Option<&'a CallService> {
        self.space_service.as_ref().or(own.first())
    }

    /// Services offered in the start menu: the space's first.
    fn services<'a>(&'a self, own: &'a [CallService]) -> Vec<&'a CallService> {
        let mut all: Vec<_> = self.space_service.iter().collect();
        all.extend(
            own.iter()
                .filter(|s| Some(*s) != self.space_service.as_ref()),
        );
        all
    }

    /// Buttons at the right of the room header.
    pub fn header_controls<'a>(&self, own: &[CallService]) -> Element<'a, app::Message> {
        let mut controls = row![].spacing(6).align_y(Alignment::Center);
        if let Some(link) = &self.link {
            let label = recognize_call_link(link.as_str(), own)
                .map_or_else(|| "Join call".to_owned(), |name| format!("Join {name}"));
            controls = controls.push(hint(
                button(row![icons::camera(14.0, on_primary), text(label).size(13)].spacing(6))
                    .padding([5, 12])
                    .style(style::primary_button)
                    .on_press(app::Message::LinkClicked(link.to_string())),
                "Open the room's call in the browser",
                Position::Bottom,
            ));
        }
        // The room's link replaces Start call; the menu still offers others.
        if self.link.is_none() {
            let start = self.default_service(own).filter(|_| !self.busy);
            let start_tip = match self.default_service(own) {
                Some(service) => format!("Start a {} call and post its link here", service.name),
                None => "Add a call service in Settings to start calls".to_owned(),
            };
            controls = controls.push(hint(
                button(text("Start call").size(13))
                    .padding([5, 10])
                    .style(style::secondary_button)
                    .on_press_maybe(start.map(|s| self.message(Message::Start(s.clone())))),
                start_tip,
                Position::Bottom,
            ));
        }
        controls
            .push(hint(
                button(text("▾").size(13))
                    .padding([5, 8])
                    .style(style::secondary_button)
                    .on_press(self.message(Message::ToggleMenu)),
                "Start a call with another service",
                Position::Bottom,
            ))
            .push(hint(
                button(text("Call link…").size(13))
                    .padding([5, 8])
                    .style(style::text_button)
                    .on_press(self.message(Message::EditLink)),
                "Set the room's standing call link",
                Position::Bottom,
            ))
            .into()
    }

    /// The start menu or link editor below the header, when open.
    pub fn panel<'a>(&'a self, own: &'a [CallService]) -> Option<Element<'a, app::Message>> {
        let content: Element<'a, app::Message> = if let Some(input) = &self.editor {
            self.editor_view(input, own)
        } else if self.menu {
            self.menu_view(own)
        } else {
            return None;
        };
        let mut panel = column![content].spacing(6);
        if let Some(error) = &self.error {
            panel = panel.push(text(error).size(12).style(text::danger));
        }
        Some(
            container(panel)
                .padding([8, 12])
                .width(Length::Fill)
                .style(style::code_block)
                .into(),
        )
    }

    fn menu_view<'a>(&'a self, own: &'a [CallService]) -> Element<'a, app::Message> {
        let services = self.services(own);
        if services.is_empty() {
            return text("No call services yet: add one in Settings.")
                .size(13)
                .into();
        }
        let mut menu = row![text("Start a call with").size(12).style(text::secondary)]
            .spacing(8)
            .align_y(Alignment::Center);
        for service in services {
            let label = if Some(service) == self.space_service.as_ref() {
                format!("{} (space)", service.name)
            } else {
                service.name.clone()
            };
            menu = menu.push(
                button(text(label).size(13))
                    .padding([4, 10])
                    .style(style::secondary_button)
                    .on_press_maybe(
                        (!self.busy).then(|| self.message(Message::Start(service.clone()))),
                    ),
            );
        }
        menu.into()
    }

    fn editor_view<'a>(
        &'a self,
        input: &'a str,
        own: &'a [CallService],
    ) -> Element<'a, app::Message> {
        let allowed = self.can_set_link != Some(false);
        let valid = Url::parse(input.trim()).is_ok();
        let field = text_input("Call link for this room, e.g. a Zoom or Jitsi link", input)
            .on_input(|v| self.message(Message::LinkInput(v)))
            .on_submit_maybe((allowed && valid).then(|| self.message(Message::SaveLink)))
            .style(style::input)
            .padding(6)
            .size(13);
        let action = |label, message: Option<Message>, primary| {
            button(text(label).size(13))
                .padding([4, 10])
                .style(if primary {
                    style::primary_button
                } else {
                    style::text_button
                })
                .on_press_maybe(message.filter(|_| !self.busy).map(|m| self.message(m)))
        };
        let mut actions = row![
            action(
                "Save",
                (allowed && valid).then_some(Message::SaveLink),
                true
            ),
            action(
                "Remove",
                (allowed && self.link.is_some()).then_some(Message::RemoveLink),
                false
            ),
            action("Cancel", Some(Message::CancelEdit), false),
            space::horizontal(),
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        // New links from services that make them without a connector.
        let generators: Vec<_> = self
            .services(own)
            .into_iter()
            .filter(|s| !matches!(s.kind, ServiceKind::GoogleMeet))
            .collect();
        if !generators.is_empty() {
            actions = actions.push(text("New link:").size(12).style(text::secondary));
            for service in generators {
                actions = actions.push(action(
                    &service.name,
                    Some(Message::NewLink(service.clone())),
                    false,
                ));
            }
        }
        let mut editor = column![field, actions].spacing(6);
        if !allowed {
            editor = editor.push(
                text("Only moderators can change this room's call link.")
                    .size(12)
                    .style(text::secondary),
            );
        }
        editor.into()
    }
}

/// Cards for the meeting links of a message, with a Join button each.
pub fn cards<'a>(links: &[&'a str], own: &[CallService]) -> Option<Element<'a, app::Message>> {
    let mut seen = Vec::new();
    let cards: Vec<Element<'a, app::Message>> = links
        .iter()
        .filter_map(|link| Some((*link, recognize_call_link(link, own)?)))
        .filter(|(link, _)| {
            let new = !seen.contains(link);
            seen.push(*link);
            new
        })
        .map(|(link, name)| card(link, name))
        .collect();
    (!cards.is_empty()).then(|| column(cards).spacing(4).into())
}

fn card<'a>(link: &'a str, service: String) -> Element<'a, app::Message> {
    let shown = link
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    container(
        row![
            container(icons::camera(18.0, on_primary))
                .center(32)
                .style(style::call_icon),
            column![
                text(format!("{service} call"))
                    .font(fonts::SEMIBOLD)
                    .size(14),
                ellipsis(shown).size(12).style(text::secondary),
            ]
            .spacing(1)
            .width(Length::Fill),
            button(text("Join").size(13))
                .padding([5, 14])
                .style(style::primary_button)
                .on_press(app::Message::LinkClicked(link.to_owned())),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding([8, 10])
    .max_width(460)
    .style(style::card)
    .into()
}

fn on_primary(theme: &Theme) -> iced::Color {
    theme.extended_palette().primary.base.text
}
