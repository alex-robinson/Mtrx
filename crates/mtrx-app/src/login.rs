//! Login screen: browser sign-in (OAuth, e.g. matrix.org) or password.

use iced::{
    Alignment, Element, Length,
    task::Handle,
    widget::{button, column, container, rule, text, text_input},
};

#[derive(Debug, Clone)]
pub enum Message {
    Homeserver(String),
    User(String),
    Password(String),
    Browser,
    SubmitPassword,
    Cancel,
}

/// What the app should do after a [`Message`].
pub enum Action {
    None,
    Browser,
    Password,
}

#[derive(Default)]
enum State {
    #[default]
    Idle,
    /// Browser sign-in running; aborting the handle cancels it.
    Browser(Handle),
    Password,
}

pub struct Login {
    pub homeserver: String,
    pub user: String,
    pub password: String,
    state: State,
    pub error: Option<String>,
}

impl Default for Login {
    fn default() -> Self {
        Self {
            homeserver: "matrix.org".into(),
            user: String::new(),
            password: String::new(),
            state: State::Idle,
            error: None,
        }
    }
}

impl Login {
    pub fn update(&mut self, message: Message) -> Action {
        match message {
            Message::Homeserver(v) => self.homeserver = v,
            Message::User(v) => self.user = v,
            Message::Password(v) => self.password = v,
            Message::Browser if self.idle() && !self.homeserver.is_empty() => {
                self.error = None;
                return Action::Browser;
            }
            Message::SubmitPassword if self.can_submit_password() => {
                self.error = None;
                self.state = State::Password;
                return Action::Password;
            }
            Message::Cancel => {
                if let State::Browser(handle) = std::mem::take(&mut self.state) {
                    handle.abort();
                }
            }
            Message::Browser | Message::SubmitPassword => {}
        }
        Action::None
    }

    pub fn browser_started(&mut self, handle: Handle) {
        self.state = State::Browser(handle);
    }

    pub fn failed(&mut self, error: String) {
        self.state = State::Idle;
        self.error = Some(error);
    }

    fn idle(&self) -> bool {
        matches!(self.state, State::Idle)
    }

    fn can_submit_password(&self) -> bool {
        self.idle()
            && !self.homeserver.is_empty()
            && !self.user.is_empty()
            && !self.password.is_empty()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let input = |placeholder, value: &str, on_input: fn(String) -> Message, submit| {
            let field = text_input(placeholder, value).padding(10);
            if self.idle() {
                field.on_input(on_input).on_submit(submit)
            } else {
                field
            }
        };

        let mut form = column![
            text("Mtrx").size(32),
            text("Sign in to your Matrix account").size(14),
            input(
                "Homeserver, e.g. matrix.org",
                &self.homeserver,
                Message::Homeserver,
                Message::Browser
            ),
        ]
        .spacing(14)
        .align_x(Alignment::Center)
        .width(360);

        form = match &self.state {
            State::Browser(_) => form.push(text("Continue in your browser…")).push(
                button(text("Cancel"))
                    .style(button::secondary)
                    .on_press(Message::Cancel),
            ),
            State::Idle | State::Password => form
                .push(
                    button(text("Sign in with browser").width(Length::Fill).center())
                        .width(Length::Fill)
                        .padding(10)
                        .on_press_maybe(self.idle().then_some(Message::Browser)),
                )
                .push(
                    text("Recommended for matrix.org and most servers")
                        .size(12)
                        .style(text::secondary),
                )
                .push(rule::horizontal(1))
                .push(text("Or with a password").size(12).style(text::secondary))
                .push(input(
                    "Username",
                    &self.user,
                    Message::User,
                    Message::SubmitPassword,
                ))
                .push(
                    input(
                        "Password",
                        &self.password,
                        Message::Password,
                        Message::SubmitPassword,
                    )
                    .secure(true),
                )
                .push(
                    button(text(if matches!(self.state, State::Password) {
                        "Signing in…"
                    } else {
                        "Sign in with password"
                    }))
                    .style(button::secondary)
                    .padding([8, 16])
                    .on_press_maybe(
                        self.can_submit_password()
                            .then_some(Message::SubmitPassword),
                    ),
                ),
        };
        if let Some(error) = &self.error {
            form = form.push(text(error).style(text::danger));
        }
        container(form).center(Length::Fill).into()
    }
}
