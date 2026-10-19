//! Files in messages: images inline, with previews downloaded in the
//! background and cached; cards for other files; the image viewer; saving
//! and sending files.

use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use iced::{
    Alignment, Element, Length, Size, Subscription, Task, Theme, event, keyboard, mouse,
    widget::{button, column, container, image, mouse_area, opaque, responsive, row, text},
    window,
};
use mtrx_core::{Media, MediaKind, MediaRef, RoomTimeline, Session};
use tokio::sync::Semaphore;

use crate::{app::Message, ellipsis::ellipsis, fonts, icons, style};

/// Largest size of an image in the timeline, in logical pixels.
const MAX_INLINE: Size = Size::new(360.0, 300.0);
/// An image's size in the timeline while its dimensions are unknown.
const PLACEHOLDER: Size = Size::new(240.0, 160.0);
/// Longest side of an image in the viewer, in pixels (GPUs limit texture
/// sizes).
const MAX_FULL: u32 = 4096;
/// Space between the viewer and the window's edges.
const VIEWER_MARGIN: f32 = 32.0;
/// Height of the viewer's bar with the filename and buttons.
const VIEWER_BAR: f32 = 44.0;
/// The viewer's bar is at least this wide, even below a narrow image.
const VIEWER_BAR_MIN_WIDTH: f32 = 360.0;
/// A failed download is tried again only after this long.
const RETRY_AFTER: Duration = Duration::from_secs(5 * 60);
/// Downloads running at once.
const PARALLEL_DOWNLOADS: usize = 3;

/// A decoded image.
#[derive(Clone, Debug)]
pub struct Picture {
    handle: image::Handle,
    /// Of the downloaded image, before downscaling.
    width: u32,
    height: u32,
}

enum Loaded {
    Ready(Picture),
    Failed(Instant),
}

/// Previews of the images in messages.
pub struct Previews {
    loaded: HashMap<MediaRef, Loaded>,
    pending: HashSet<MediaRef>,
    downloads: Arc<Semaphore>,
}

impl Previews {
    pub fn new() -> Self {
        Self {
            loaded: HashMap::new(),
            pending: HashSet::new(),
            downloads: Arc::new(Semaphore::new(PARALLEL_DOWNLOADS)),
        }
    }

    fn ready(&self, media: &MediaRef) -> Option<&Picture> {
        match self.loaded.get(media)? {
            Loaded::Ready(picture) => Some(picture),
            Loaded::Failed(_) => None,
        }
    }

    fn failed(&self, media: &MediaRef) -> bool {
        matches!(self.loaded.get(media), Some(Loaded::Failed(_)))
    }

    /// Start downloading unless it is cached, running, or failed recently.
    pub fn request<M: Send + 'static>(
        &mut self,
        session: &Arc<Session>,
        media: &MediaRef,
        done: fn(MediaRef, Option<Picture>) -> M,
    ) -> Task<M> {
        let wanted = match self.loaded.get(media) {
            Some(Loaded::Ready(_)) => false,
            Some(Loaded::Failed(at)) => at.elapsed() >= RETRY_AFTER,
            None => true,
        };
        if !wanted || !self.pending.insert(media.clone()) {
            return Task::none();
        }
        let (session, downloads, media) = (session.clone(), self.downloads.clone(), media.clone());
        Task::perform(
            async move {
                let picture = async {
                    // Never closed, so this only waits.
                    let _permit = downloads.acquire().await.ok()?;
                    let bytes = session.media_preview(&media).await.ok()?;
                    tokio::task::spawn_blocking(move || decode_preview(&bytes))
                        .await
                        .ok()?
                }
                .await;
                (media, picture)
            },
            move |(media, picture)| done(media, picture),
        )
    }

    /// Store a download's result; `None` if it failed.
    pub fn insert(&mut self, media: MediaRef, picture: Option<Picture>) {
        self.pending.remove(&media);
        let loaded = match picture {
            Some(picture) => Loaded::Ready(picture),
            None => Loaded::Failed(Instant::now()),
        };
        self.loaded.insert(media, loaded);
    }
}

#[cfg(test)]
impl Previews {
    /// Download and decode right away (tests).
    pub async fn load_now(&mut self, session: &Session, media: &MediaRef) {
        let bytes = session.media_preview(media).await.unwrap();
        let picture = decode_preview(&bytes);
        assert!(picture.is_some(), "decoding failed");
        self.insert(media.clone(), picture);
    }
}

/// Decoded at up to twice the size shown, for HiDPI screens.
fn decode_preview(bytes: &[u8]) -> Option<Picture> {
    decode(
        bytes,
        (2.0 * MAX_INLINE.width) as u32,
        (2.0 * MAX_INLINE.height) as u32,
    )
}

/// Decode, shrunk to fit `max_width`×`max_height` pixels.
fn decode(bytes: &[u8], max_width: u32, max_height: u32) -> Option<Picture> {
    let picture = ::image::load_from_memory(bytes).ok()?;
    let (width, height) = (picture.width(), picture.height());
    let picture = if width > max_width || height > max_height {
        picture.resize(
            max_width,
            max_height,
            ::image::imageops::FilterType::Triangle,
        )
    } else {
        picture
    };
    let rgba = picture.into_rgba8();
    Some(Picture {
        handle: image::Handle::from_rgba(rgba.width(), rgba.height(), rgba.into_raw()),
        width,
        height,
    })
}

/// `width`×`height` shrunk to fit `bounds`, keeping its aspect ratio.
fn fit(width: u32, height: u32, bounds: Size) -> Size {
    let (width, height) = (width.max(1) as f32, height.max(1) as f32);
    let scale = (bounds.width / width).min(bounds.height / height).min(1.0);
    Size::new((width * scale).round(), (height * scale).round())
}

/// Pixel size of an image: from its message, else from the picture.
fn dimensions(media: &Media, picture: Option<&Picture>) -> Option<(u32, u32)> {
    media
        .width
        .zip(media.height)
        .or(picture.map(|p| (p.width, p.height)))
}

/// A message's file: an image (a placeholder until loaded), else a card.
/// `body` shows below it if it is a caption rather than the filename.
pub fn view<'a>(media: &'a Media, body: &'a str, previews: &Previews) -> Element<'a, Message> {
    let content = match media.kind {
        MediaKind::Image if !previews.failed(&media.source) => {
            inline_image(media, previews.ready(&media.source))
        }
        _ => card(media),
    };
    if body == media.filename {
        content
    } else {
        column![content, text(body)].spacing(4).into()
    }
}

/// An image in the timeline; clicking it opens the viewer.
fn inline_image<'a>(media: &'a Media, picture: Option<&Picture>) -> Element<'a, Message> {
    let size = dimensions(media, picture).map_or(PLACEHOLDER, |(w, h)| fit(w, h, MAX_INLINE));
    let content: Element<'a, Message> = match picture {
        Some(picture) => image(picture.handle.clone())
            .width(size.width)
            .height(size.height)
            .border_radius(style::MEDIA_RADIUS)
            .into(),
        None => placeholder(size),
    };
    mouse_area(content)
        .on_press(Message::OpenImage(media.clone()))
        .interaction(mouse::Interaction::Pointer)
        .into()
}

fn placeholder<'a>(size: Size) -> Element<'a, Message> {
    container(text("Loading image…").size(12).style(text::secondary))
        .center_x(size.width)
        .center_y(size.height)
        .style(style::media_placeholder)
        .into()
}

/// A file: icon, name, size and type, and a Save button.
fn card<'a>(media: &'a Media) -> Element<'a, Message> {
    let details = match media.size {
        Some(size) => format!("{} · {}", human_size(size), kind_label(media)),
        None => kind_label(media),
    };
    container(
        row![
            container(icons::file(16.0, on_primary))
                .center(32)
                .style(style::call_icon),
            column![
                ellipsis(&media.filename).font(fonts::SEMIBOLD).size(14),
                text(details).size(12).style(text::secondary),
            ]
            .spacing(1)
            .width(Length::Fill),
            save_button(media),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding([8, 10])
    .max_width(460)
    .style(style::card)
    .into()
}

fn save_button<'a>(media: &Media) -> Element<'a, Message> {
    button(text("Save…").size(13))
        .padding([5, 12])
        .style(style::secondary_button)
        .on_press(Message::SaveMedia(media.clone()))
        .into()
}

/// "Video", "Audio", or for other files their extension, e.g. "PDF".
fn kind_label(media: &Media) -> String {
    match media.kind {
        MediaKind::Image => "Image".into(),
        MediaKind::Video => "Video".into(),
        MediaKind::Audio => "Audio".into(),
        MediaKind::File => Path::new(&media.filename)
            .extension()
            .and_then(|e| e.to_str())
            .filter(|e| !e.is_empty() && e.len() <= 5)
            .map_or_else(|| "File".into(), str::to_uppercase),
    }
}

/// A file size in decimal units, e.g. "2.4 MB".
fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["kB", "MB", "GB", "TB"];
    if bytes < 1000 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1000.0;
    let mut unit = 0;
    while value >= 999.95 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if value < 99.95 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}

fn on_primary(theme: &Theme) -> iced::Color {
    theme.extended_palette().primary.base.text
}

/// An image shown large over the window.
pub struct Viewer {
    pub media: Media,
    /// The whole image, once loaded; until then the preview shows.
    full: Option<Picture>,
    pub error: Option<String>,
}

impl Viewer {
    pub fn new(media: Media) -> Self {
        Self {
            media,
            full: None,
            error: None,
        }
    }

    /// Download and decode the whole image.
    pub fn load(&self, session: Arc<Session>) -> Task<Message> {
        let media = self.media.source.clone();
        Task::perform(
            {
                let media = media.clone();
                async move {
                    let bytes = session
                        .media_content(&media)
                        .await
                        .map_err(|e| e.to_string())?;
                    tokio::task::spawn_blocking(move || decode(&bytes, MAX_FULL, MAX_FULL))
                        .await
                        .ok()
                        .flatten()
                        .ok_or_else(|| "not an image we can show".to_owned())
                }
            },
            move |result| Message::ImageLoaded(media.clone(), result),
        )
    }

    /// The whole image of `media` was loaded, or not.
    pub fn loaded(&mut self, media: &MediaRef, result: Result<Picture, String>) {
        if *media != self.media.source {
            return;
        }
        match result {
            Ok(picture) => self.full = Some(picture),
            Err(e) => self.error = Some(format!("Could not load the image: {e}")),
        }
    }

    /// The image as large as fits (at most its own size), with its name,
    /// Save and Close below. Clicking around it closes it.
    pub fn view<'a>(&'a self, previews: &'a Previews) -> Element<'a, Message> {
        let picture = self.full.as_ref().or(previews.ready(&self.media.source));
        let content = responsive(move |available| {
            let bounds = Size::new(
                available.width,
                (available.height - VIEWER_BAR - 8.0).max(0.0),
            );
            let size =
                dimensions(&self.media, picture).map_or(PLACEHOLDER, |(w, h)| fit(w, h, bounds));
            let shown: Element<'a, Message> = match picture {
                Some(picture) => image(picture.handle.clone())
                    .width(size.width)
                    .height(size.height)
                    .border_radius(style::MEDIA_RADIUS)
                    .into(),
                None => placeholder(size),
            };
            let bar_width = size.width.max(VIEWER_BAR_MIN_WIDTH).min(available.width);
            container(
                column![opaque(shown), opaque(self.bar(bar_width))]
                    .spacing(8)
                    .align_x(Alignment::Center),
            )
            .center(Length::Fill)
            .into()
        });
        mouse_area(
            container(content)
                .padding(VIEWER_MARGIN)
                .style(style::viewer_backdrop),
        )
        .on_press(Message::CloseImage)
        .into()
    }

    fn bar<'a>(&'a self, width: f32) -> Element<'a, Message> {
        let mut name = column![
            ellipsis(&self.media.filename)
                .font(fonts::SEMIBOLD)
                .size(14)
        ]
        .width(Length::Fill);
        if let Some(error) = &self.error {
            name = name.push(ellipsis(error).size(12).style(text::danger));
        }
        container(
            row![
                name,
                save_button(&self.media),
                button(text("Close").size(13))
                    .padding([5, 12])
                    .style(style::primary_button)
                    .on_press(Message::CloseImage),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding([0, 12])
        .center_y(VIEWER_BAR)
        .width(width)
        .style(style::dialog)
        .into()
    }
}

#[cfg(test)]
impl Viewer {
    /// Load the whole image right away (tests).
    pub async fn load_now(&mut self, session: &Session) {
        let bytes = session.media_content(&self.media.source).await.unwrap();
        self.full = decode(&bytes, MAX_FULL, MAX_FULL);
        assert!(self.full.is_some(), "decoding failed");
    }
}

/// Ask where to save a message's file, then download it there. Cancelling
/// is fine.
pub async fn save(session: Arc<Session>, media: Media) -> Result<(), String> {
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_file_name(&media.filename)
        .save_file()
        .await
    else {
        return Ok(());
    };
    let bytes = session
        .media_content(&media.source)
        .await
        .map_err(|e| format!("Could not download {}: {e}", media.filename))?;
    file.write(&bytes)
        .await
        .map_err(|e| format!("Could not save {}: {e}", file.file_name()))
}

/// Ask for files and send them to the timeline's room.
pub async fn attach(timeline: Arc<RoomTimeline>) -> Result<(), String> {
    let Some(files) = rfd::AsyncFileDialog::new()
        .set_title("Attach files")
        .pick_files()
        .await
    else {
        return Ok(());
    };
    for file in files {
        send(&timeline, file.path()).await?;
    }
    Ok(())
}

/// Send the file at `path` to the timeline's room.
pub async fn send(timeline: &RoomTimeline, path: &Path) -> Result<(), String> {
    timeline
        .send_file(path)
        .await
        .map_err(|e| format!("Could not send {}: {e}", path.display()))
}

/// Esc, while the viewer is open.
pub fn escape() -> Subscription<Message> {
    event::listen_with(|event, _status, _window| match event {
        iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Escape),
            ..
        }) => Some(Message::CloseImage),
        _ => None,
    })
}

/// Files dropped onto the window.
pub fn dropped_files() -> Subscription<Message> {
    event::listen_with(|event, _status, _window| match event {
        iced::Event::Window(window::Event::FileDropped(path)) => Some(Message::FileDropped(path)),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(999), "999 B");
        assert_eq!(human_size(1000), "1.0 kB");
        assert_eq!(human_size(2_400_000), "2.4 MB");
        assert_eq!(human_size(99_960), "100 kB");
        assert_eq!(human_size(999_960), "1.0 MB");
        assert_eq!(human_size(345_000_000), "345 MB");
    }

    #[test]
    fn fitting() {
        assert_eq!(fit(640, 400, MAX_INLINE), Size::new(360.0, 225.0));
        assert_eq!(fit(400, 800, MAX_INLINE), Size::new(150.0, 300.0));
        // Small images keep their size.
        assert_eq!(fit(48, 32, MAX_INLINE), Size::new(48.0, 32.0));
    }
}
