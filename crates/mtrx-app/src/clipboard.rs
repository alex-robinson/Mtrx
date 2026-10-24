//! The system clipboard beyond iced's plain text: text copied from the
//! timeline goes as HTML too, so that Mail or a document keeps its
//! formatting, links and lists; pictures and files pasted into the
//! composer are attached.

use std::{io::Cursor, path::PathBuf, sync::Mutex};

use arboard::ImageData;
use iced::widget::text_editor::{Binding, KeyPress};
use mtrx_core::Attachment;
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

/// What ⌘V in the composer pastes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pasted {
    /// Files copied in the Finder or Explorer: attached, as if dropped.
    Files(Vec<PathBuf>),
    /// A picture, such as a screenshot: attached, as a PNG.
    Image(Attachment),
    /// Text: typed in.
    Text(String),
}

/// Whether `press` pastes into an editor (⌘V, while focused).
pub fn pastes(press: &KeyPress) -> bool {
    matches!(
        Binding::<()>::from_key_press(press.clone()),
        Some(Binding::Paste)
    )
}

/// What the clipboard holds, as pasted into the composer; `None` for
/// nothing it can take.
pub async fn read() -> Option<Pasted> {
    tokio::task::spawn_blocking(|| {
        with(|clipboard| {
            let files = clipboard.get().file_list().unwrap_or_default();
            let text = clipboard.get().text().ok();
            Ok(pasted(files, text, || clipboard.get().image().ok()))
        })
        .ok()
        .flatten()
    })
    .await
    .ok()
    .flatten()
}

/// What is pasted of the clipboard's `files`, `text` and `image`, in that
/// order: copied files come with their icon as a picture, and text from
/// some apps (Word) with a picture of it.
fn pasted(
    files: Vec<PathBuf>,
    text: Option<String>,
    image: impl FnOnce() -> Option<ImageData<'static>>,
) -> Option<Pasted> {
    if !files.is_empty() {
        return Some(Pasted::Files(files));
    }
    if let Some(text) = text.filter(|text| !text.is_empty()) {
        return Some(Pasted::Text(text));
    }
    image().and_then(png).map(Pasted::Image)
}

/// A picture from the clipboard as a PNG file to attach.
fn png(image: ImageData<'_>) -> Option<Attachment> {
    let picture = image::RgbaImage::from_raw(
        image.width as u32,
        image.height as u32,
        image.bytes.into_owned(),
    )?;
    let mut bytes = Vec::new();
    picture
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
        .ok()?;
    Some(Attachment {
        filename: "image.png".into(),
        mimetype: "image/png".into(),
        bytes,
    })
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

#[cfg(test)]
mod tests {
    use iced::{
        Event, Size,
        keyboard::{self, Key, Modifiers},
        mouse,
        widget::text_editor::{self, Content},
    };
    use iced_test::simulator::{Simulator, press_key};

    use super::*;

    /// `key` pressed with `modifiers`.
    fn press(key: &str, modifiers: Modifiers) -> Event {
        match press_key(Key::Character(key.into()), None) {
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modified_key,
                physical_key,
                location,
                text,
                repeat,
                ..
            }) => Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modified_key,
                physical_key,
                location,
                modifiers,
                text,
                repeat,
            }),
            _ => unreachable!(),
        }
    }

    #[derive(Clone, Debug, PartialEq)]
    enum Pressed {
        Edit,
        Paste,
    }

    /// What an editor bound as the composer is (see `room.rs`) sends for
    /// `events`, focused by a click first if `focused`.
    fn editor_sends(focused: bool, events: Vec<Event>) -> Vec<Pressed> {
        let content = Content::<iced::Renderer>::with_text("Melt");
        let editor: iced::Element<'_, Pressed> = iced::widget::text_editor(&content)
            .key_binding(|press| {
                if pastes(&press) {
                    Some(text_editor::Binding::Custom(Pressed::Paste))
                } else {
                    text_editor::Binding::from_key_press(press)
                }
            })
            .on_action(|_| Pressed::Edit)
            .into();
        let mut ui =
            Simulator::with_size(crate::fonts::settings(), Size::new(300.0, 100.0), editor);
        if focused {
            ui.point_at((290.0, 10.0));
            let _ = ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(
                mouse::Button::Left,
            ))]);
        }
        let _ = ui.simulate(events);
        ui.into_messages()
            .filter(|pressed| *pressed != Pressed::Edit)
            .collect()
    }

    #[test]
    fn command_v_pastes_through_us() {
        let command_v = || press("v", Modifiers::COMMAND);
        assert_eq!(editor_sends(true, vec![command_v()]), [Pressed::Paste]);
        // Not while another field has the focus.
        assert!(editor_sends(false, vec![command_v()]).is_empty());
    }

    fn picture() -> ImageData<'static> {
        ImageData {
            width: 2,
            height: 1,
            bytes: vec![255, 0, 0, 255, 0, 0, 255, 128].into(),
        }
    }

    #[test]
    fn pastes_files_then_text_then_a_picture() {
        let file = PathBuf::from("/tmp/notes.pdf");
        let no_picture = || -> Option<ImageData<'static>> { panic!("not read") };
        assert_eq!(
            pasted(vec![file.clone()], Some("notes.pdf".into()), no_picture),
            Some(Pasted::Files(vec![file]))
        );
        assert_eq!(
            pasted(Vec::new(), Some("Melt".into()), no_picture),
            Some(Pasted::Text("Melt".into()))
        );
        let Some(Pasted::Image(file)) = pasted(Vec::new(), Some(String::new()), || Some(picture()))
        else {
            panic!("a picture is attached");
        };
        assert_eq!(file.mimetype, "image/png");
        // The same pixels.
        let decoded = image::load_from_memory(&file.bytes).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (2, 1));
        assert_eq!(decoded.into_raw(), picture().bytes.into_owned());
        assert_eq!(pasted(Vec::new(), None, || None), None);
    }
}
