//! App preferences, shared by all accounts, in `settings.json` next to the
//! profiles, and the settings panel.

use std::{
    fmt, io,
    path::{Path, PathBuf},
};

use iced::{
    Alignment, Element, Length,
    task::Handle,
    widget::{
        button, column, container, pick_list, row, scrollable, space, text, text_input, toggler,
    },
};
use mtrx_core::{CallService, GoogleClient, OwnedRoomId, ServiceKind};
use serde::{Deserialize, Serialize};

use crate::{app, fonts, style};

const FILE: &str = "settings.json";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Senders' names in their colour (as in Element) instead of the text colour.
    pub colored_names: bool,
    /// Services for starting calls; the first is the default.
    pub call_services: Vec<CallService>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            colored_names: false,
            call_services: vec![
                CallService::jitsi("Jitsi Meet", "https://meet.jit.si").expect("valid"),
            ],
        }
    }
}

impl Settings {
    pub fn path(data_root: &Path) -> PathBuf {
        data_root.join(FILE)
    }

    /// The saved settings; defaults if there are none or they cannot be read.
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_vec_pretty(self)?)
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    ColoredNames(bool),
    FormKind(FormKind),
    FormName(String),
    FormUrl(String),
    AddService,
    MakeDefault(usize),
    RemoveService(usize),
    /// The selected space's call service and whether we may change it.
    SpaceLoaded(OwnedRoomId, Option<CallService>, Option<bool>),
    SetSpaceService(SpaceChoice),
    SpaceSaved(Result<(), String>),
    GoogleClientId(String),
    GoogleClientSecret(String),
    GoogleSignIn,
    GoogleCancel,
    /// The signed-in account's email.
    GoogleSignedIn(Result<String, String>),
    GoogleSignOut,
    GoogleSignedOut(Result<(), String>),
}

/// What the app should do after a [`Message`].
pub enum Action {
    None,
    /// The settings changed; save them.
    Save,
    SetSpaceService(OwnedRoomId, Option<CallService>),
    GoogleSignIn(GoogleClient),
    GoogleSignOut,
}

/// State of the open settings panel.
#[derive(Default)]
pub struct Panel {
    /// Saving failed.
    pub error: Option<String>,
    form: ServiceForm,
    /// The selected space, while one is.
    pub space: Option<SpaceCalls>,
    pub google: GoogleState,
}

/// The Google connection, for Meet links.
#[derive(Default)]
pub struct GoogleState {
    /// The signed-in account's email.
    pub account: Option<String>,
    client_id: String,
    client_secret: String,
    /// Browser sign-in running; aborting the handle cancels it.
    pub signing_in: Option<Handle>,
    pub error: Option<String>,
}

/// The call service of a space, as shown in the panel.
pub struct SpaceCalls {
    pub id: OwnedRoomId,
    pub name: String,
    /// `None` until loaded.
    service: Option<Option<CallService>>,
    can_set: Option<bool>,
    error: Option<String>,
}

impl SpaceCalls {
    pub fn new(id: OwnedRoomId, name: String) -> Self {
        Self {
            id,
            name,
            service: None,
            can_set: None,
            error: None,
        }
    }
}

#[derive(Default)]
struct ServiceForm {
    kind: FormKind,
    name: String,
    url: String,
    error: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FormKind {
    #[default]
    Jitsi,
    Rooms,
    Fixed,
    GoogleMeet,
}

impl FormKind {
    const ALL: [Self; 4] = [Self::Jitsi, Self::Rooms, Self::Fixed, Self::GoogleMeet];

    fn url_placeholder(self) -> &'static str {
        match self {
            Self::Jitsi => "Server, e.g. https://meet.jit.si",
            Self::Rooms => "Link with {room}, e.g. https://p2p.mirotalk.com/join/{room}",
            Self::Fixed => "Meeting link, e.g. https://zoom.us/j/123456789",
            Self::GoogleMeet => "",
        }
    }
}

impl fmt::Display for FormKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Jitsi => "Jitsi server",
            Self::Rooms => "New room per call",
            Self::Fixed => "Fixed link",
            Self::GoogleMeet => "Google Meet",
        })
    }
}

/// An entry of the space service picker.
#[derive(Clone, Debug, PartialEq)]
pub struct SpaceChoice(pub Option<CallService>);

impl fmt::Display for SpaceChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Some(service) => f.write_str(&service.name),
            None => f.write_str("None"),
        }
    }
}

impl Panel {
    pub fn update(&mut self, settings: &mut Settings, message: Message) -> Action {
        match message {
            Message::ColoredNames(colored) => {
                settings.colored_names = colored;
                return Action::Save;
            }
            Message::FormKind(kind) => self.form.kind = kind,
            Message::FormName(name) => self.form.name = name,
            Message::FormUrl(url) => self.form.url = url,
            Message::AddService => match self.form.service() {
                Ok(service) => {
                    settings.call_services.push(service);
                    self.form = ServiceForm::default();
                    return Action::Save;
                }
                Err(e) => self.form.error = Some(e),
            },
            Message::MakeDefault(index) if index < settings.call_services.len() => {
                let service = settings.call_services.remove(index);
                settings.call_services.insert(0, service);
                return Action::Save;
            }
            Message::RemoveService(index) if index < settings.call_services.len() => {
                settings.call_services.remove(index);
                return Action::Save;
            }
            Message::MakeDefault(_) | Message::RemoveService(_) => {}
            Message::SpaceLoaded(id, service, can_set) => {
                if let Some(space) = self.space.as_mut().filter(|s| s.id == id) {
                    space.service = Some(service);
                    space.can_set = can_set;
                }
            }
            Message::SetSpaceService(SpaceChoice(service)) => {
                if let Some(space) = &mut self.space {
                    space.error = None;
                    space.service = Some(service.clone());
                    return Action::SetSpaceService(space.id.clone(), service);
                }
            }
            Message::SpaceSaved(result) => {
                if let Some(space) = &mut self.space {
                    space.error = result.err();
                }
            }
            Message::GoogleClientId(id) => self.google.client_id = id,
            Message::GoogleClientSecret(secret) => self.google.client_secret = secret,
            Message::GoogleSignIn if self.google.can_sign_in() => {
                self.google.error = None;
                return Action::GoogleSignIn(GoogleClient {
                    client_id: self.google.client_id.trim().to_owned(),
                    client_secret: self.google.client_secret.trim().to_owned(),
                });
            }
            Message::GoogleSignIn => {}
            Message::GoogleCancel => {
                if let Some(handle) = self.google.signing_in.take() {
                    handle.abort();
                }
            }
            Message::GoogleSignedIn(result) => {
                self.google.signing_in = None;
                match result {
                    Ok(account) => {
                        self.google.account = Some(account);
                        self.google.client_secret.clear();
                        // Offer Meet links right away.
                        if !settings
                            .call_services
                            .iter()
                            .any(|s| s.kind == ServiceKind::GoogleMeet)
                        {
                            settings.call_services.push(CallService::google_meet());
                            return Action::Save;
                        }
                    }
                    Err(e) => self.google.error = Some(e),
                }
            }
            Message::GoogleSignOut => {
                self.google.error = None;
                return Action::GoogleSignOut;
            }
            Message::GoogleSignedOut(result) => match result {
                Ok(()) => self.google.account = None,
                Err(e) => self.google.error = Some(e),
            },
        }
        Action::None
    }

    pub fn view<'a>(&'a self, settings: &'a Settings) -> Element<'a, app::Message> {
        let mut panel = column![
            row![
                text("Settings").size(18).font(fonts::SEMIBOLD),
                space::horizontal(),
                button(text("Done"))
                    .style(style::primary_button)
                    .on_press(app::Message::ShowSettings(false)),
            ]
            .align_y(Alignment::Center),
            heading("Chat"),
            option(
                "Coloured names",
                "Show people's names in their own colour, as Element does.",
                toggler(settings.colored_names)
                    .on_toggle(|on| message(Message::ColoredNames(on)))
                    .into(),
            ),
            heading("Calls"),
            text(
                "Start call uses the space's call service if it has one, \
                 otherwise the first one here."
            )
            .size(12)
            .style(text::secondary),
            self.services_view(settings),
            self.form.view(),
        ]
        .spacing(14);
        panel = panel.push(heading("Google Meet")).push(self.google.view());
        if let Some(space) = &self.space {
            panel = panel.push(heading("Space")).push(space.view(settings));
        }
        if let Some(error) = &self.error {
            panel = panel.push(
                text(format!("Could not save settings: {error}"))
                    .size(12)
                    .style(text::danger),
            );
        }
        container(scrollable(container(panel).padding(20)))
            .width(560)
            .max_height(640)
            .style(style::dialog)
            .into()
    }

    fn services_view<'a>(&self, settings: &'a Settings) -> Element<'a, app::Message> {
        if settings.call_services.is_empty() {
            return text("No call services.").size(13).into();
        }
        column(
            settings
                .call_services
                .iter()
                .enumerate()
                .map(|(i, service)| {
                    let detail = match &service.kind {
                        ServiceKind::Rooms { template } => template.as_str(),
                        ServiceKind::Fixed { url } => url.as_str(),
                        ServiceKind::GoogleMeet => "A new meeting with your Google account",
                    };
                    let mut line = row![
                        column![
                            text(&service.name),
                            text(detail).size(12).style(text::secondary)
                        ]
                        .width(Length::Fill),
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center);
                    if i == 0 {
                        line = line.push(text("Default").size(12).style(text::secondary));
                    } else {
                        line = line.push(small_button(
                            "Make default",
                            message(Message::MakeDefault(i)),
                        ));
                    }
                    line.push(small_button("Remove", message(Message::RemoveService(i))))
                        .into()
                }),
        )
        .spacing(8)
        .into()
    }
}

impl GoogleState {
    fn can_sign_in(&self) -> bool {
        self.signing_in.is_none()
            && !self.client_id.trim().is_empty()
            && !self.client_secret.trim().is_empty()
    }

    fn view(&self) -> Element<'_, app::Message> {
        let mut section = column![].spacing(8);
        if let Some(account) = &self.account {
            section = section.push(
                row![
                    text(format!("Connected as {account}")).width(Length::Fill),
                    small_button("Disconnect", message(Message::GoogleSignOut)),
                ]
                .align_y(Alignment::Center),
            );
        } else if self.signing_in.is_some() {
            section = section.push(
                row![
                    text("Continue in your browser…").width(Length::Fill),
                    small_button("Cancel", message(Message::GoogleCancel)),
                ]
                .align_y(Alignment::Center),
            );
        } else {
            let field = |placeholder, value: &str, on_input: fn(String) -> Message| {
                text_input(placeholder, value)
                    .on_input(move |v| message(on_input(v)))
                    .on_submit(message(Message::GoogleSignIn))
                    .style(style::input)
                    .padding(6)
                    .size(13)
            };
            section = section
                .push(
                    text(
                        "Creates Meet links with your Google account. Needs an OAuth client \
                         of your own Google Cloud project (see the README).",
                    )
                    .size(12)
                    .style(text::secondary),
                )
                .push(
                    row![
                        field("Client ID", &self.client_id, Message::GoogleClientId),
                        field(
                            "Client secret",
                            &self.client_secret,
                            Message::GoogleClientSecret
                        )
                        .secure(true),
                        button(text("Connect").size(13))
                            .padding([5, 12])
                            .style(style::secondary_button)
                            .on_press_maybe(
                                self.can_sign_in().then(|| message(Message::GoogleSignIn))
                            ),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                );
        }
        if let Some(error) = &self.error {
            section = section.push(text(error).size(12).style(text::danger));
        }
        section.into()
    }
}

impl ServiceForm {
    /// The service described by the form.
    fn service(&self) -> Result<CallService, String> {
        let url = self.url.trim();
        let name = match self.name.trim() {
            "" => self.kind.to_string(),
            name => name.to_owned(),
        };
        let service = match self.kind {
            FormKind::Jitsi => CallService::jitsi(&name, url),
            FormKind::Rooms => CallService::rooms(&name, url),
            FormKind::Fixed => CallService::fixed(&name, url),
            FormKind::GoogleMeet => Ok(CallService::google_meet()),
        };
        service.map_err(|e| e.to_string())
    }

    fn view(&self) -> Element<'_, app::Message> {
        let field = |placeholder, value: &str, on_input: fn(String) -> Message| {
            text_input(placeholder, value)
                .on_input(move |v| message(on_input(v)))
                .on_submit(message(Message::AddService))
                .style(style::input)
                .padding(6)
                .size(13)
        };
        let mut inputs = row![
            pick_list(FormKind::ALL, Some(self.kind), |k| message(
                Message::FormKind(k)
            ))
            .text_size(13)
            .padding(6),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        if self.kind != FormKind::GoogleMeet {
            inputs = inputs
                .push(field("Name", &self.name, Message::FormName).width(120))
                .push(field(
                    self.kind.url_placeholder(),
                    &self.url,
                    Message::FormUrl,
                ));
        } else {
            inputs = inputs.push(
                text("Needs the Google connection below.")
                    .size(12)
                    .style(text::secondary)
                    .width(Length::Fill),
            );
        }
        inputs = inputs.push(
            button(text("Add").size(13))
                .padding([5, 12])
                .style(style::secondary_button)
                .on_press(message(Message::AddService)),
        );
        let mut form = column![inputs].spacing(4);
        if let Some(error) = &self.error {
            form = form.push(text(error).size(12).style(text::danger));
        }
        form.into()
    }
}

impl SpaceCalls {
    fn view<'a>(&'a self, settings: &'a Settings) -> Element<'a, app::Message> {
        let mut section =
            column![text(format!("Call service for everyone in {}", self.name))].spacing(6);
        match &self.service {
            None => section = section.push(text("Loading…").size(12).style(text::secondary)),
            Some(current) => {
                let mut choices = vec![SpaceChoice(None)];
                choices.extend(current.iter().cloned().map(|s| SpaceChoice(Some(s))));
                choices.extend(
                    settings
                        .call_services
                        .iter()
                        .filter(|s| Some(*s) != current.as_ref())
                        .cloned()
                        .map(|s| SpaceChoice(Some(s))),
                );
                let allowed = self.can_set != Some(false);
                let picker =
                    pick_list(choices, Some(SpaceChoice(current.clone())), move |choice| {
                        message(Message::SetSpaceService(choice))
                    })
                    .text_size(13)
                    .padding(6);
                section = section.push(if allowed {
                    Element::from(picker)
                } else {
                    row![
                        text(SpaceChoice(current.clone()).to_string()),
                        text("Only moderators can change it.")
                            .size(12)
                            .style(text::secondary),
                    ]
                    .spacing(12)
                    .into()
                });
            }
        }
        if let Some(error) = &self.error {
            section = section.push(text(error).size(12).style(text::danger));
        }
        section.into()
    }
}

fn heading<'a>(label: &str) -> Element<'a, app::Message> {
    text(label.to_uppercase())
        .size(11)
        .font(fonts::SEMIBOLD)
        .style(text::secondary)
        .into()
}

fn option<'a>(
    label: &'a str,
    description: &'a str,
    toggle: Element<'a, app::Message>,
) -> Element<'a, app::Message> {
    row![
        column![
            text(label),
            text(description).size(12).style(text::secondary)
        ]
        .spacing(2)
        .width(Length::Fill),
        toggle,
    ]
    .spacing(16)
    .align_y(Alignment::Center)
    .into()
}

fn small_button<'a>(label: &'a str, on_press: app::Message) -> Element<'a, app::Message> {
    button(text(label).size(12))
        .padding([2, 8])
        .style(style::text_button)
        .on_press(on_press)
        .into()
}

fn message(message: Message) -> app::Message {
    app::Message::Settings(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_and_loads() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = Settings::path(dir.path());
        assert_eq!(Settings::load(&path), Settings::default(), "missing file");
        let settings = Settings {
            colored_names: true,
            call_services: vec![CallService::fixed("Zoom", "https://zoom.us/j/1").unwrap()],
        };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path), settings);
        std::fs::write(&path, "{}").unwrap();
        assert_eq!(Settings::load(&path), Settings::default(), "older file");
    }

    #[test]
    fn adds_services_from_the_form() {
        let mut settings = Settings::default();
        let mut panel = Panel::default();
        for message in [
            Message::FormKind(FormKind::Fixed),
            Message::FormName("My Zoom".into()),
            Message::FormUrl("https://zoom.us/j/42".into()),
        ] {
            panel.update(&mut settings, message);
        }
        assert!(matches!(
            panel.update(&mut settings, Message::AddService),
            Action::Save
        ));
        assert_eq!(settings.call_services[1].name, "My Zoom");
        panel.update(&mut settings, Message::MakeDefault(1));
        assert_eq!(settings.call_services[0].name, "My Zoom");

        panel.update(&mut settings, Message::FormUrl("not a link".into()));
        assert!(matches!(
            panel.update(&mut settings, Message::AddService),
            Action::None
        ));
        assert!(panel.form.error.is_some());
    }
}
