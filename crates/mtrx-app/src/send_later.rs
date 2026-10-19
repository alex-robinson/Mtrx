//! Send later: the bar of scheduled messages above the composer, and the
//! picker for a message's send time.

use std::time::{Duration, SystemTime};

use iced::{
    Alignment, Background, Border, Element, Length, Theme,
    widget::{button, column, container, row, space, text, text::Wrapping, text_input},
};
use mtrx_core::{ScheduledMessage, describe_send_time, parse_send_time};

use crate::{app, style};

#[derive(Debug, Clone)]
pub enum Message {
    TogglePicker,
    /// The typed custom time changed.
    Custom(String),
    SubmitCustom,
    /// Schedule the composer's text (or save the edited message) for this time.
    At(SystemTime),
    Edit(ScheduledMessage),
    SendNow(String),
    Cancel(String),
    /// The composer's text was scheduled or the edit saved.
    Saved(Result<(), String>),
    /// Sending now or cancelling finished.
    Done(Result<(), String>),
}

/// Send-later state of an open room.
#[derive(Default)]
pub struct SendLater {
    /// The time picker is open, with this custom time typed.
    pub picker: Option<String>,
}

impl SendLater {
    /// The send time typed into the picker, if it reads as one.
    pub fn custom_time(&self, now: SystemTime) -> Option<SystemTime> {
        self.picker
            .as_deref()
            .and_then(|custom| parse_send_time(custom, now))
            .filter(|at| *at > now)
    }
}

/// A room's scheduled messages, earliest first, as of `now`.
pub struct Pending<'a> {
    pub messages: Vec<&'a ScheduledMessage>,
    pub now: SystemTime,
}

/// The bar of a room's scheduled messages.
pub fn queue<'a>(
    pending: &Pending<'a>,
    editing: Option<&str>,
    composer_empty: bool,
) -> Element<'a, app::Message> {
    let (messages, now) = (&pending.messages, pending.now);
    column(messages.iter().map(|message| {
        // Editing replaces the composer's text, so only from an empty composer.
        let can_edit = editing.is_none() && composer_empty;
        item(message, editing == Some(message.id.as_str()), can_edit, now)
    }))
    .spacing(4)
    .into()
}

fn item<'a>(
    message: &'a ScheduledMessage,
    editing: bool,
    can_edit: bool,
    now: SystemTime,
) -> Element<'a, app::Message> {
    let when = describe_send_time(message.send_at, now);
    let status = if message.overdue {
        text(format!("Overdue · was due {when}")).font(crate::fonts::SEMIBOLD)
    } else if editing {
        text(format!("Editing · {when}"))
    } else {
        text(format!("{when} · {}", until(message.send_at, now)))
    };
    let preview = message.text.lines().next().unwrap_or_default().to_owned();

    let action = |label, on_press: Option<Message>, primary: bool| {
        button(text(label).size(12))
            .padding([2, 8])
            .style(if primary {
                style::primary_button
            } else {
                style::text_button
            })
            .on_press_maybe(on_press.map(app::Message::Later))
    };
    let edit_label = if message.overdue {
        "Reschedule"
    } else {
        "Edit"
    };
    let actions = row![
        action(
            edit_label,
            can_edit.then(|| Message::Edit(message.clone())),
            false
        ),
        action(
            "Send now",
            (!editing).then(|| Message::SendNow(message.id.clone())),
            message.overdue,
        ),
        action(
            "Cancel",
            (!editing).then(|| Message::Cancel(message.id.clone())),
            false
        ),
    ]
    .spacing(2)
    .align_y(Alignment::Center);

    let content = row![
        status.size(12).width(Length::Shrink),
        text(preview)
            .size(13)
            .wrapping(Wrapping::None)
            .width(Length::Fill),
        actions,
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    let kind = match (message.overdue, editing) {
        (true, _) => Kind::Overdue,
        (false, true) => Kind::Editing,
        (false, false) => Kind::Waiting,
    };
    container(content)
        .padding([4, 10])
        .width(Length::Fill)
        .clip(true)
        .style(move |theme| item_style(theme, kind))
        .into()
}

/// The time picker: presets and a typed time.
pub fn picker<'a>(
    state: &'a SendLater,
    can_schedule: bool,
    now: SystemTime,
) -> Option<Element<'a, app::Message>> {
    let custom = state.picker.as_deref()?;
    let preset = |label: &'a str, at: Option<SystemTime>| {
        button(text(label).size(13))
            .padding([4, 10])
            .style(style::secondary_button)
            .on_press_maybe(
                at.filter(|_| can_schedule)
                    .map(|at| app::Message::Later(Message::At(at))),
            )
    };
    let presets = row![
        text("Send at").size(12).style(text::secondary),
        preset("In 1 hour", Some(now + Duration::from_secs(3600))),
        preset("Tomorrow 09:00", parse_send_time("tomorrow", now)),
        preset("Monday 09:00", parse_send_time("mon", now)),
        space::horizontal(),
        button(text("Close").size(12))
            .padding([2, 8])
            .style(style::text_button)
            .on_press(app::Message::Later(Message::TogglePicker)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let parsed = state.custom_time(now);
    let preview = match parsed {
        Some(at) => text(format!("→ {}", describe_send_time(at, now))).size(13),
        None if custom.trim().is_empty() => text(""),
        None => text("Not a time in the future")
            .size(13)
            .style(text::danger),
    };
    let input = text_input(
        "Or type a time: 17:30, tomorrow 8:00, fri 14:00, in 2h",
        custom,
    )
    .on_input(|v| app::Message::Later(Message::Custom(v)))
    .on_submit_maybe(
        (can_schedule && parsed.is_some()).then_some(app::Message::Later(Message::SubmitCustom)),
    )
    .size(13)
    .padding(6)
    .width(360)
    .style(style::input);

    let picker = column![
        presets,
        row![input, preview].spacing(12).align_y(Alignment::Center)
    ]
    .spacing(8);
    Some(
        container(picker)
            .padding(10)
            .width(Length::Fill)
            .style(|theme| item_style(theme, Kind::Picker))
            .into(),
    )
}

/// "in 5 min", "in 2 h 10 min", "in 3 days".
fn until(at: SystemTime, now: SystemTime) -> String {
    let minutes = at
        .duration_since(now)
        .unwrap_or_default()
        .as_secs()
        .div_ceil(60);
    match minutes {
        0 => "now".into(),
        1..60 => format!("in {minutes} min"),
        60..1440 if minutes.is_multiple_of(60) => format!("in {} h", minutes / 60),
        60..1440 => format!("in {} h {} min", minutes / 60, minutes % 60),
        _ => match (minutes + 720) / 1440 {
            1 => "in 1 day".into(),
            days => format!("in {days} days"),
        },
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Waiting,
    Editing,
    Overdue,
    Picker,
}

/// Colour hints: neutral while waiting, accent while edited, warning when overdue.
fn item_style(theme: &Theme, kind: Kind) -> container::Style {
    let palette = theme.extended_palette();
    let (background, edge, text_color) = match kind {
        Kind::Waiting | Kind::Picker => (
            palette.background.weak.color,
            palette.background.strong.color,
            None,
        ),
        Kind::Editing => (
            palette.primary.weak.color.scale_alpha(0.35),
            palette.primary.base.color,
            None,
        ),
        Kind::Overdue => (
            palette.warning.weak.color,
            palette.warning.strong.color,
            Some(palette.warning.weak.text),
        ),
    };
    container::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: Border::default().rounded(6).width(1).color(edge),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_time_left() {
        let now = SystemTime::UNIX_EPOCH;
        let after = |secs| now + Duration::from_secs(secs);
        assert_eq!(until(after(0), now), "now");
        assert_eq!(until(after(30), now), "in 1 min");
        assert_eq!(until(after(45 * 60), now), "in 45 min");
        assert_eq!(until(after(2 * 3600), now), "in 2 h");
        assert_eq!(until(after(2 * 3600 + 600), now), "in 2 h 10 min");
        assert_eq!(until(after(30 * 3600), now), "in 1 day");
        assert_eq!(until(after(40 * 3600), now), "in 2 days");
    }
}
