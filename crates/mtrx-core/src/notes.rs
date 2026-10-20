//! Notes to self as Markdown, e.g. to take into a writing app.

use chrono::{DateTime, Local, NaiveDate};

use crate::{EventContent, TimelineEntry};

/// Each note of a timeline under a heading with its time, oldest first;
/// only from `since` (local day) on. Messages come as written (Markdown),
/// files by name.
pub fn notes_markdown(entries: &[TimelineEntry], since: Option<NaiveDate>) -> String {
    let mut out = String::new();
    for entry in entries {
        let TimelineEntry::Event(event) = entry else {
            continue;
        };
        let EventContent::Message(message) = &event.content else {
            continue;
        };
        let at = DateTime::<Local>::from(event.timestamp);
        if since.is_some_and(|since| at.date_naive() < since) {
            continue;
        }
        let text = match &message.media {
            Some(media) if message.body == media.filename => format!("[{}]", media.filename),
            Some(media) => format!("[{}]\n\n{}", media.filename, message.body),
            None => message.body.clone(),
        };
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&format!(
            "## {}\n\n{}\n",
            at.format("%Y-%m-%d %H:%M"),
            text.trim_end()
        ));
    }
    out
}
