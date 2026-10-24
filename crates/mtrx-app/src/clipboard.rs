//! The system clipboard beyond iced's plain text: text copied from the
//! timeline goes as HTML too, so that Mail or a document keeps its
//! formatting, links and lists.

use std::sync::Mutex;

use mtrx_format::{Block, Document, Inline, Style};

/// What copying text puts on the clipboard.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Copied {
    /// Plain text, math as `$…$`.
    pub text: String,
    /// The same as HTML, math as `$…$` too.
    pub html: String,
}

impl Copied {
    /// A line saying who wrote what follows, and when.
    pub fn byline(name: &str, time: &str) -> Self {
        let line = Document {
            blocks: vec![Block::Paragraph(vec![
                Inline::Styled {
                    style: Style::Bold,
                    content: vec![Inline::Text(name.to_owned())],
                },
                Inline::Text(format!(", {time}")),
            ])],
        };
        Self {
            text: format!("{name}, {time}\n"),
            html: line.to_html(),
        }
    }

    /// `next` right after this.
    pub fn then(mut self, next: Copied) -> Self {
        self.text.push_str(&next.text);
        self.html.push_str(&next.html);
        self
    }

    /// `next` after this, a paragraph apart.
    pub fn push(&mut self, next: Copied) {
        if !self.text.is_empty() {
            self.text.push_str("\n\n");
        }
        self.text.push_str(&next.text);
        self.html.push_str(&next.html);
    }
}

/// Put `copied` on the clipboard: its HTML, with its text for apps that
/// take only text.
pub async fn write(copied: Copied) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        with(|clipboard| clipboard.set().html(copied.html, Some(copied.text)))
            .map_err(|e| format!("Could not copy: {e}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Runs `f` on the app's clipboard. One for as long as the app runs: on
/// X11 what we copied is served by it, and gone once it is dropped.
fn with<T>(
    f: impl FnOnce(&mut arboard::Clipboard) -> Result<T, arboard::Error>,
) -> Result<T, arboard::Error> {
    static CLIPBOARD: Mutex<Option<arboard::Clipboard>> = Mutex::new(None);
    let mut clipboard = CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner());
    let clipboard = match &mut *clipboard {
        Some(clipboard) => clipboard,
        none => none.insert(arboard::Clipboard::new()?),
    };
    f(clipboard)
}
