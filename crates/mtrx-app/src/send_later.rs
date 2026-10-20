//! Send later: the bar of scheduled messages above the composer, and the
//! picker for a message's send time: a calendar, hour and minute wheels,
//! quick picks, and a typed time, all kept in step.

use std::time::{Duration, SystemTime};

use chrono::{DateTime, Datelike, Local, Months, NaiveDate, NaiveTime, TimeZone, Timelike};
use iced::{
    Alignment, Background, Border, Element, Length, Task, Theme,
    widget::{
        Id, button, column, container, operation, row, scrollable, space, text, text::Wrapping,
        text_input,
    },
};
use mtrx_core::{ScheduledMessage, describe_send_time, parse_send_time};

use crate::{app, fonts, style};

/// The wheels' scrollables, to bring the picked hour and minute into view.
const HOURS: &str = "send-later-hours";
const MINUTES: &str = "send-later-minutes";
const MINUTE_STEP: u32 = 5;
/// Height of a wheel entry, and how many show.
const WHEEL_ROW: f32 = 26.0;
const WHEEL_ROWS: f32 = 7.0;
const DAY_CELL: f32 = 32.0;

#[derive(Debug, Clone)]
pub enum Message {
    TogglePicker,
    /// The typed time changed.
    Typed(String),
    /// Show the month this many months away.
    Month(i32),
    Day(NaiveDate),
    Hour(u32),
    Minute(u32),
    /// Schedule for the picked time.
    Schedule,
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
    /// The time picker, while open.
    pub picker: Option<Picker>,
}

/// A send time being picked. The typed text is what gets scheduled; the
/// calendar and wheels write it, and typing a valid time moves them.
pub struct Picker {
    pub typed: String,
    /// The month shown, as its first day.
    month: NaiveDate,
    date: NaiveDate,
    hour: u32,
    minute: u32,
}

impl Picker {
    /// Starting at `at`, or at the next full hour.
    pub fn new(now: SystemTime, at: Option<SystemTime>) -> Self {
        let start = at
            .filter(|at| *at > now)
            .map(DateTime::<Local>::from)
            .unwrap_or_else(|| next_hour(DateTime::from(now)));
        let mut picker = Self {
            typed: String::new(),
            month: first_of_month(start.date_naive()),
            date: start.date_naive(),
            hour: start.hour(),
            minute: start.minute(),
        };
        picker.typed = picker.formatted();
        picker
    }

    fn formatted(&self) -> String {
        format!(
            "{} {:02}:{:02}",
            self.date.format("%Y-%m-%d"),
            self.hour,
            self.minute
        )
    }

    /// The time to schedule, or why there is none.
    pub fn time(&self, now: SystemTime) -> Result<SystemTime, &'static str> {
        match parse_send_time(&self.typed, now) {
            _ if self.typed.trim().is_empty() => Err("Pick or type a time"),
            None => Err("Not a time"),
            Some(at) if at <= now => Err("That time has passed"),
            Some(at) => Ok(at),
        }
    }

    pub fn update(&mut self, message: &Message, now: SystemTime) -> Task<app::Message> {
        let today = DateTime::<Local>::from(now).date_naive();
        match message {
            Message::Typed(typed) => {
                self.typed = typed.clone();
                if let Some(at) = parse_send_time(typed, now) {
                    let at = DateTime::<Local>::from(at);
                    self.date = at.date_naive();
                    self.month = first_of_month(self.date);
                    self.hour = at.hour();
                    self.minute = at.minute();
                    return self.scroll();
                }
                return Task::none();
            }
            Message::Month(by) => {
                let month = if *by < 0 {
                    self.month
                        .checked_sub_months(Months::new(by.unsigned_abs()))
                } else {
                    self.month.checked_add_months(Months::new(*by as u32))
                };
                if let Some(month) = month.filter(|m| *m >= first_of_month(today)) {
                    self.month = month;
                }
                return Task::none();
            }
            Message::Day(date) => self.date = *date,
            Message::Hour(hour) => self.hour = *hour,
            Message::Minute(minute) => self.minute = *minute,
            _ => return Task::none(),
        }
        self.typed = self.formatted();
        Task::none()
    }

    /// Bring the picked hour and minute into view.
    pub fn scroll(&self) -> Task<app::Message> {
        let offset = |index: u32| operation::AbsoluteOffset {
            x: None,
            y: Some((index as f32 - (WHEEL_ROWS - 1.0) / 2.0).max(0.0) * WHEEL_ROW),
        };
        Task::batch([
            operation::scroll_to(HOURS, offset(self.hour)),
            operation::scroll_to(MINUTES, offset(self.minute / MINUTE_STEP)),
        ])
    }

    /// A month of days, Monday first; past days are disabled.
    fn calendar<'a>(&self, today: NaiveDate) -> Element<'a, app::Message> {
        let nav = |label: &'static str, by: i32, enabled: bool| {
            button(text(label).size(14))
                .padding([0, 8])
                .style(style::text_button)
                .on_press_maybe(enabled.then_some(app::Message::Later(Message::Month(by))))
        };
        let header = row![
            nav("‹", -1, self.month > first_of_month(today)),
            text(self.month.format("%B %Y").to_string())
                .size(13)
                .font(fonts::SEMIBOLD)
                .width(Length::Fill)
                .center(),
            nav("›", 1, true),
        ]
        .align_y(Alignment::Center);
        let weekdays = row(["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"].map(|day| {
            text(day)
                .size(11)
                .style(text::secondary)
                .width(DAY_CELL)
                .center()
                .into()
        }));
        let mut weeks = column![header, weekdays].spacing(2).width(DAY_CELL * 7.0);
        let lead = self.month.weekday().num_days_from_monday();
        let mut day = self.month - chrono::Days::new(lead.into());
        while day.month() == self.month.month() || day < self.month {
            let mut week = row![];
            for _ in 0..7 {
                week = week.push(if day.month() == self.month.month() {
                    self.day_button(day, today)
                } else {
                    space().width(DAY_CELL).into()
                });
                day = day.succ_opt().unwrap_or(day);
            }
            weeks = weeks.push(week);
        }
        weeks.into()
    }

    fn day_button<'a>(&self, day: NaiveDate, today: NaiveDate) -> Element<'a, app::Message> {
        let label = text(day.day().to_string()).size(12).center();
        let label = if day == today {
            label.font(fonts::SEMIBOLD)
        } else {
            label
        };
        let selected = day == self.date;
        button(label.width(Length::Fill))
            .width(DAY_CELL)
            .padding([4, 0])
            .style(move |theme, status| style::wheel_item(theme, status, selected))
            .on_press_maybe((day >= today).then_some(app::Message::Later(Message::Day(day))))
            .into()
    }

    /// A scrolling column of hours or minutes; past ones are disabled.
    fn wheel<'a>(
        &self,
        id: &'static str,
        values: impl Iterator<Item = u32>,
        picked: u32,
        allowed: impl Fn(u32) -> bool,
        message: fn(u32) -> Message,
    ) -> Element<'a, app::Message> {
        let entries = values.map(|value| {
            let selected = value == picked;
            button(
                text(format!("{value:02}"))
                    .size(13)
                    .center()
                    .width(Length::Fill),
            )
            .height(WHEEL_ROW)
            .padding([4, 0])
            .style(move |theme, status| style::wheel_item(theme, status, selected))
            .on_press_maybe(allowed(value).then(|| app::Message::Later(message(value))))
            .into()
        });
        scrollable(column(entries).width(48))
            .id(Id::new(id))
            .height(WHEEL_ROW * WHEEL_ROWS)
            .direction(scrollable::Direction::Vertical(
                scrollable::Scrollbar::new().width(0).scroller_width(0),
            ))
            .into()
    }
}

fn next_hour(now: DateTime<Local>) -> DateTime<Local> {
    let hour = now + chrono::Duration::hours(1);
    let start = NaiveTime::from_hms_opt(hour.hour(), 0, 0).unwrap_or_default();
    Local
        .from_local_datetime(&hour.date_naive().and_time(start))
        .earliest()
        .unwrap_or(hour)
}

fn first_of_month(date: NaiveDate) -> NaiveDate {
    date.with_day(1).unwrap_or(date)
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

/// The time picker: calendar, hour and minute wheels, quick picks and a
/// typed time.
pub fn picker<'a>(
    state: &'a SendLater,
    can_schedule: bool,
    now: SystemTime,
) -> Option<Element<'a, app::Message>> {
    let picker = state.picker.as_ref()?;
    let local = DateTime::<Local>::from(now);
    let today = local.date_naive();
    let quick = |label: &'a str, at: Option<SystemTime>| {
        button(text(label).size(12))
            .padding([3, 8])
            .style(style::secondary_button)
            .on_press_maybe(
                at.filter(|_| can_schedule)
                    .map(|at| app::Message::Later(Message::At(at))),
            )
    };
    let quick_picks = row![
        quick("In 1 hour", Some(now + Duration::from_secs(3600))),
        quick("Tomorrow 09:00", parse_send_time("tomorrow", now)),
        quick("Monday 09:00", parse_send_time("mon", now)),
    ]
    .spacing(6)
    .wrap();

    // Past hours and minutes of today are not offered.
    let on_today = picker.date == today;
    let hours = picker.wheel(
        HOURS,
        0..24,
        picker.hour,
        move |hour| !on_today || hour >= local.hour(),
        Message::Hour,
    );
    let this_hour = on_today && picker.hour == local.hour();
    let minutes = picker.wheel(
        MINUTES,
        (0..60).step_by(MINUTE_STEP as usize),
        picker.minute,
        move |minute| !this_hour || minute > local.minute(),
        Message::Minute,
    );

    let time = picker.time(now);
    let status = match time {
        Ok(at) => text(format!(
            "{} · {}",
            describe_send_time(at, now),
            until(at, now)
        ))
        .size(13),
        Err(problem) => text(problem).size(13).style(text::danger),
    };
    let schedule = (can_schedule && time.is_ok()).then_some(app::Message::Later(Message::Schedule));
    let input = text_input("17:30, tomorrow 8:00, fri 14:00, in 2h", &picker.typed)
        .on_input(|v| app::Message::Later(Message::Typed(v)))
        .on_submit_maybe(schedule.clone())
        .size(13)
        .padding(6)
        .style(style::input);
    let side = column![
        text("Send at").size(12).font(fonts::SEMIBOLD),
        quick_picks,
        input,
        status,
        space::vertical(),
        row![
            button(text("Schedule").size(13))
                .padding([5, 12])
                .style(style::primary_button)
                .on_press_maybe(schedule),
            button(text("Close").size(13))
                .padding([5, 12])
                .style(style::text_button)
                .on_press(app::Message::Later(Message::TogglePicker)),
        ]
        .spacing(8),
    ]
    .spacing(8)
    .width(Length::Fill)
    .height(WHEEL_ROW * WHEEL_ROWS + 50.0);

    let content = row![
        picker.calendar(today),
        column![text("Hour").size(11).style(text::secondary), hours,]
            .spacing(4)
            .align_x(Alignment::Center),
        column![text("Min").size(11).style(text::secondary), minutes,]
            .spacing(4)
            .align_x(Alignment::Center),
        side,
    ]
    .spacing(16);
    Some(
        container(content)
            .padding(12)
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

    fn local(date: (i32, u32, u32), hour: u32, minute: u32) -> SystemTime {
        Local
            .with_ymd_and_hms(date.0, date.1, date.2, hour, minute, 0)
            .unwrap()
            .into()
    }

    #[test]
    fn calendar_wheels_and_typing_agree() {
        let now = local((2026, 10, 20), 14, 23);
        let mut picker = Picker::new(now, None);
        // The next full hour.
        assert_eq!(picker.typed, "2026-10-20 15:00");
        assert_eq!(picker.time(now), Ok(local((2026, 10, 20), 15, 0)));

        let _ = picker.update(
            &Message::Day(NaiveDate::from_ymd_opt(2026, 11, 2).unwrap()),
            now,
        );
        let _ = picker.update(&Message::Hour(9), now);
        let _ = picker.update(&Message::Minute(30), now);
        assert_eq!(picker.typed, "2026-11-02 09:30");
        assert_eq!(picker.time(now), Ok(local((2026, 11, 2), 9, 30)));

        let _ = picker.update(&Message::Typed("tomorrow 8:15".into()), now);
        assert_eq!((picker.date.day(), picker.hour, picker.minute), (21, 8, 15));
        assert_eq!(picker.month, NaiveDate::from_ymd_opt(2026, 10, 1).unwrap());

        let _ = picker.update(&Message::Typed("yesterday".into()), now);
        assert_eq!(picker.time(now), Err("Not a time"));
        let _ = picker.update(&Message::Typed("10:00".into()), now);
        // 10:00 has passed today, so it means tomorrow.
        assert_eq!(picker.time(now), Ok(local((2026, 10, 21), 10, 0)));

        // No months before this one.
        let _ = picker.update(&Message::Month(-1), now);
        assert_eq!(picker.month, NaiveDate::from_ymd_opt(2026, 10, 1).unwrap());
        let _ = picker.update(&Message::Month(3), now);
        assert_eq!(picker.month, NaiveDate::from_ymd_opt(2027, 1, 1).unwrap());
    }

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
