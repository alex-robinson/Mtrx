//! Calls through links: call cards in the timeline, the room header's call
//! controls: one button joining the room's call link or, without one,
//! starting a call; and a menu for calls with other services and for
//! setting the room's link.

use iced::{
    Alignment, Element, Length, Theme,
    widget::{button, checkbox, column, container, row, rule, space, text, text_input},
};
use mtrx_core::{CallService, OwnedRoomId, ServiceKind, Url, recognize_call_link};

use crate::{
    app,
    ellipsis::ellipsis,
    fonts,
    hint::{Position, hint},
    icons,
    popover::{Edge, Side, popover},
    style,
};

const MENU_WIDTH: f32 = 230.0;
const EDITOR_WIDTH: f32 = 520.0;

#[derive(Debug, Clone)]
pub enum Message {
    /// The room's call link changed.
    Link(Option<Url>),
    /// Whether we may change the room's call link; `None` if unknown.
    CanSetLink(Option<bool>),
    /// The call service of the room's nearest space with one.
    SpaceService(Option<CallService>),
    OpenMenu,
    /// Close the menu or the link editor.
    Close,
    /// Post a new call link of this service in the room.
    Start(CallService),
    Started(Result<(), String>),
    EditLink,
    LinkInput(String),
    /// Whether saving the link also posts it in the room.
    Announce(bool),
    /// Fill the link editor with a new link of this service.
    NewLink(CallService),
    SaveLink,
    RemoveLink,
    LinkSaved(Result<(), String>),
}

/// Call state of the open room.
pub struct RoomCalls {
    room: OwnedRoomId,
    pub link: Option<Url>,
    pub can_set_link: Option<bool>,
    pub space_service: Option<CallService>,
    pub menu: bool,
    /// The call link being edited, in a panel below the menu button.
    pub editor: Option<String>,
    /// Post the saved link in the room too, for other clients.
    pub announce: bool,
    /// A call is being started or the link saved.
    pub busy: bool,
    /// Saving the link failed.
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
            announce: false,
            busy: false,
            error: None,
        }
    }

    /// The menu or the link editor is open.
    pub fn is_open(&self) -> bool {
        self.menu || self.editor.is_some()
    }

    /// Close the menu or the link editor.
    pub fn close(&mut self) {
        self.menu = false;
        self.editor = None;
        self.error = None;
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

    /// Buttons at the right of the room header: Join or Start call, and
    /// the menu, which also holds the link editor.
    pub fn header_controls<'a>(&'a self, own: &'a [CallService]) -> Element<'a, app::Message> {
        let open = self.is_open();
        // Hints would cover the open menu.
        let tip = |control: Element<'a, app::Message>, label: String| {
            if open {
                control
            } else {
                hint(control, label, Position::Bottom)
            }
        };
        let main = match &self.link {
            Some(link) => {
                let label = recognize_call_link(link.as_str(), own)
                    .map_or_else(|| "Join call".to_owned(), |name| format!("Join {name}"));
                tip(
                    button(row![icons::camera(14.0, on_primary), text(label).size(13)].spacing(6))
                        .padding([5, 12])
                        .style(style::primary_button)
                        .on_press(app::Message::LinkClicked(link.to_string()))
                        .into(),
                    "Open the room's call in the browser".to_owned(),
                )
            }
            None => {
                let service = self.default_service(own);
                let label = match service {
                    Some(service) => {
                        format!(
                            "Start a {} call now: posts a new link in the chat",
                            service.name
                        )
                    }
                    None => "Add a call service in Settings to start calls".to_owned(),
                };
                tip(
                    button(text("Start call").size(13))
                        .padding([5, 10])
                        .style(style::secondary_button)
                        .on_press_maybe(
                            service
                                .filter(|_| !self.busy)
                                .map(|s| self.message(Message::Start(s.clone()))),
                        )
                        .into(),
                    label,
                )
            }
        };
        let panel = match &self.editor {
            Some(input) => Some(self.editor_view(input, own)),
            None => self.menu.then(|| self.menu_view(own)),
        };
        // While open, a click on it closes the panel as any outside click.
        let toggle = tip(
            button(text("▾").size(13))
                .padding([5, 8])
                .style(style::secondary_button)
                .on_press_maybe((!open).then(|| self.message(Message::OpenMenu)))
                .into(),
            "Other call services, and the room's call link".to_owned(),
        );
        row![
            main,
            popover(toggle, panel, Side::Below, Edge::Right)
                .on_dismiss(self.message(Message::Close)),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .into()
    }

    fn menu_view<'a>(&'a self, own: &'a [CallService]) -> Element<'a, app::Message> {
        let item = |label: String, message: Option<Message>| {
            button(ellipsis(label).size(13))
                .width(Length::Fill)
                .padding([4, 10])
                .style(style::list_item(false))
                .on_press_maybe(message.filter(|_| !self.busy).map(|m| self.message(m)))
        };
        let heading =
            |label| container(text(label).size(12).style(text::secondary)).padding([4, 10]);
        let mut menu = column![heading("Start a new call with")].spacing(1);
        let services = self.services(own);
        if services.is_empty() {
            menu = menu.push(heading("No call services yet: add one in Settings"));
        }
        for service in services {
            let label = if Some(service) == self.space_service.as_ref() {
                format!("{} (space)", service.name)
            } else {
                service.name.clone()
            };
            menu = menu.push(item(label, Some(Message::Start(service.clone()))));
        }
        menu = menu.push(container(rule::horizontal(1).style(style::separator)).padding([4, 0]));
        let allowed = self.can_set_link != Some(false);
        menu = menu.push(item(
            if self.link.is_some() {
                "Change room call link…"
            } else {
                "Set room call link…"
            }
            .to_owned(),
            Some(Message::EditLink),
        ));
        if self.link.is_some() && allowed {
            menu = menu.push(item(
                "Remove room call link".to_owned(),
                Some(Message::RemoveLink),
            ));
        }
        if let Some(error) = &self.error {
            menu = menu.push(container(text(error).size(12).style(text::danger)).padding([4, 10]));
        }
        container(menu)
            .width(MENU_WIDTH)
            .padding(4)
            .style(style::toolbar)
            .into()
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
            action("Cancel", Some(Message::Close), false),
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
        let announce = checkbox(self.announce)
            .label("Also post the link in the room, for people using other apps")
            .size(14)
            .text_size(12)
            .on_toggle_maybe((!self.busy).then_some(|on| self.message(Message::Announce(on))));
        let mut editor = column![field, announce, actions].spacing(6);
        if !allowed {
            editor = editor.push(
                text("Only moderators can change this room's call link.")
                    .size(12)
                    .style(text::secondary),
            );
        }
        if let Some(error) = &self.error {
            editor = editor.push(text(error).size(12).style(text::danger));
        }
        container(editor)
            .width(EDITOR_WIDTH)
            .padding([8, 12])
            .style(style::toolbar)
            .into()
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
