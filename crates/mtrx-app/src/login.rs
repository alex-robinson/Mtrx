//! Login screen.

use iced::{
    Alignment, Element, Length,
    widget::{button, column, container, text, text_input},
};

#[derive(Debug, Clone)]
pub enum Message {
    Homeserver(String),
    User(String),
    Password(String),
    Submit,
}

pub struct Login {
    pub homeserver: String,
    pub user: String,
    pub password: String,
    pub busy: bool,
    pub error: Option<String>,
}

impl Default for Login {
    fn default() -> Self {
        Self {
            homeserver: "matrix.org".into(),
            user: String::new(),
            password: String::new(),
            busy: false,
            error: None,
        }
    }
}

impl Login {
    /// Returns `true` when the form should be submitted.
    pub fn update(&mut self, message: Message) -> bool {
        match message {
            Message::Homeserver(v) => self.homeserver = v,
            Message::User(v) => self.user = v,
            Message::Password(v) => self.password = v,
            Message::Submit => return self.can_submit(),
        }
        false
    }

    fn can_submit(&self) -> bool {
        !self.busy
            && !self.homeserver.is_empty()
            && !self.user.is_empty()
            && !self.password.is_empty()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let input = |placeholder, value: &str, on_input: fn(String) -> Message| {
            let field = text_input(placeholder, value).padding(10);
            if self.busy {
                field
            } else {
                field.on_input(on_input).on_submit(Message::Submit)
            }
        };
        let submit = button(text(if self.busy {
            "Signing in…"
        } else {
            "Sign in"
        }))
        .padding([10, 20])
        .on_press_maybe(self.can_submit().then_some(Message::Submit));

        let mut form = column![
            text("Mtrx").size(32),
            text("Sign in to your Matrix account").size(14),
            input(
                "Homeserver, e.g. matrix.org",
                &self.homeserver,
                Message::Homeserver
            ),
            input("Username", &self.user, Message::User),
            input("Password", &self.password, Message::Password).secure(true),
            submit,
        ]
        .spacing(14)
        .align_x(Alignment::Center)
        .width(360);
        if let Some(error) = &self.error {
            form = form.push(text(error).style(text::danger));
        }
        container(form).center(Length::Fill).into()
    }
}
