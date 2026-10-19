//! User, room and space pictures. Thumbnails are downloaded in the background
//! and cached as images; until one is loaded, or without a picture, the
//! initial letter on a colour derived from the ID shows instead.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};

use iced::{
    Background, Border, Color, Element, Task, Theme,
    widget::{container, image, text},
};
use mtrx_core::{MxcUri, OwnedMxcUri, Session};
use tokio::sync::Semaphore;
use unicode_segmentation::UnicodeSegmentation;

use crate::fonts;

/// Element's colours for avatars without a picture and for display names.
const PALETTE: [Color; 8] = [
    iced::color!(0x368bd6),
    iced::color!(0xac3ba8),
    iced::color!(0x0dbd8b),
    iced::color!(0xe64f7a),
    iced::color!(0xff812d),
    iced::color!(0x2dc2c5),
    iced::color!(0x5c56f5),
    iced::color!(0x74d12c),
];

/// A failed download is tried again only after this long.
const RETRY_AFTER: Duration = Duration::from_secs(10 * 60);
/// Downloads running at once; a fresh room list asks for many.
const PARALLEL_DOWNLOADS: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// Users and rooms.
    Circle,
    /// Spaces.
    RoundedSquare,
}

/// One picture at one pixel size.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    mxc: OwnedMxcUri,
    pixels: u32,
}

enum Loaded {
    Image(image::Handle),
    Failed(Instant),
}

pub struct Avatars {
    images: HashMap<Key, Loaded>,
    pending: HashSet<Key>,
    downloads: Arc<Semaphore>,
}

impl Avatars {
    pub fn new() -> Self {
        Self {
            images: HashMap::new(),
            pending: HashSet::new(),
            downloads: Arc::new(Semaphore::new(PARALLEL_DOWNLOADS)),
        }
    }

    /// Thumbnails have twice the logical size, for HiDPI screens.
    pub fn key(mxc: &MxcUri, size: f32) -> Key {
        Key {
            mxc: mxc.to_owned(),
            pixels: (size * 2.0).round() as u32,
        }
    }

    fn get(&self, mxc: &MxcUri, size: f32) -> Option<&image::Handle> {
        match self.images.get(&Self::key(mxc, size))? {
            Loaded::Image(handle) => Some(handle),
            Loaded::Failed(_) => None,
        }
    }

    /// Start downloading unless it is cached, running, or failed recently.
    pub fn request<M: Send + 'static>(
        &mut self,
        session: &Arc<Session>,
        mxc: &MxcUri,
        size: f32,
        done: fn(Key, Option<image::Handle>) -> M,
    ) -> Task<M> {
        let key = Self::key(mxc, size);
        let wanted = match self.images.get(&key) {
            Some(Loaded::Image(_)) => false,
            Some(Loaded::Failed(at)) => at.elapsed() >= RETRY_AFTER,
            None => true,
        };
        if !wanted || !self.pending.insert(key.clone()) {
            return Task::none();
        }
        let (session, downloads) = (session.clone(), self.downloads.clone());
        Task::perform(
            async move {
                let handle = async {
                    // Never closed, so this only waits.
                    let _permit = downloads.acquire().await.ok()?;
                    let bytes = session.thumbnail(&key.mxc, key.pixels).await.ok()?;
                    let pixels = key.pixels;
                    tokio::task::spawn_blocking(move || decode(&bytes, pixels))
                        .await
                        .ok()?
                }
                .await;
                (key, handle)
            },
            move |(key, handle)| done(key, handle),
        )
    }

    /// Store a download's result; `None` if it failed.
    pub fn insert(&mut self, key: Key, handle: Option<image::Handle>) {
        self.pending.remove(&key);
        let loaded = match handle {
            Some(handle) => Loaded::Image(handle),
            None => Loaded::Failed(Instant::now()),
        };
        self.images.insert(key, loaded);
    }

    /// The picture if it is loaded, else the initial of `name` on the colour
    /// of `id` (a user or room ID).
    pub fn view<'a, M: 'a>(
        &self,
        id: &str,
        name: &str,
        mxc: Option<&MxcUri>,
        size: f32,
        shape: Shape,
    ) -> Element<'a, M> {
        let radius = match shape {
            Shape::Circle => size / 2.0,
            Shape::RoundedSquare => size / 4.0,
        };
        if let Some(handle) = mxc.and_then(|mxc| self.get(mxc, size)) {
            return image(handle.clone())
                .width(size)
                .height(size)
                .border_radius(radius)
                .into();
        }
        let background = color(id);
        container(
            text(initial(name))
                .size((size * 0.55).round())
                .font(fonts::SEMIBOLD)
                .color(Color::WHITE),
        )
        .center(size)
        .style(move |_| container::Style {
            background: Some(Background::Color(background)),
            border: Border::default().rounded(radius),
            ..Default::default()
        })
        .into()
    }
}

/// Decode a thumbnail and crop it to a `pixels` square.
fn decode(bytes: &[u8], pixels: u32) -> Option<image::Handle> {
    let picture = ::image::load_from_memory(bytes).ok()?.resize_to_fill(
        pixels,
        pixels,
        ::image::imageops::FilterType::Triangle,
    );
    let rgba = picture.into_rgba8();
    Some(image::Handle::from_rgba(
        rgba.width(),
        rgba.height(),
        rgba.into_raw(),
    ))
}

#[cfg(test)]
impl Avatars {
    /// Download and decode right away (tests).
    pub async fn load_now(&mut self, session: &Session, mxc: &MxcUri, size: f32) {
        let key = Self::key(mxc, size);
        let bytes = session.thumbnail(mxc, key.pixels).await.unwrap();
        let handle = decode(&bytes, key.pixels);
        assert!(handle.is_some(), "decoding failed");
        self.insert(key, handle);
    }
}

/// Colour for a user or room ID, the same as Element picks.
pub fn color(id: &str) -> Color {
    PALETTE[(element_hash(id) % PALETTE.len() as u32) as usize]
}

/// Text contrast names need against the background (WCAG AA).
const MIN_CONTRAST: f32 = 4.5;

/// Colour of a user's name: their [`color`], darkened on light backgrounds
/// until it reads well. Some of Element's colours (lime, cyan) are too light
/// on white.
pub fn name_color(id: &str, theme: &Theme) -> Color {
    let palette = theme.extended_palette();
    let color = color(id);
    if palette.is_dark {
        color
    } else {
        darken_to_contrast(color, palette.background.base.color)
    }
}

fn darken_to_contrast(mut color: Color, background: Color) -> Color {
    while contrast(color, background) < MIN_CONTRAST && luminance(color) > 0.0 {
        color = Color {
            r: color.r * 0.95,
            g: color.g * 0.95,
            b: color.b * 0.95,
            ..color
        };
    }
    color
}

/// WCAG contrast ratio, from 1 (none) to 21.
fn contrast(a: Color, b: Color) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// WCAG relative luminance of an sRGB colour.
fn luminance(color: Color) -> f32 {
    let linear = |v: f32| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
}

/// Element's string hash (Java's `hashCode` over UTF-16 code units, made
/// positive).
fn element_hash(s: &str) -> u32 {
    s.encode_utf16()
        .fold(0i32, |hash, unit| {
            hash.wrapping_shl(5)
                .wrapping_sub(hash)
                .wrapping_add(i32::from(unit))
        })
        .unsigned_abs()
}

/// First character of a name as the user sees it (a whole grapheme, so
/// emoji stay intact), without the sigil of an ID (`@alice:…`, `#room:…`).
fn initial(name: &str) -> String {
    let name = name
        .strip_prefix(['@', '#', '!', '+'])
        .filter(|rest| !rest.is_empty())
        .unwrap_or(name);
    name.graphemes(true)
        .next()
        .map(|g| g.to_uppercase())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_matches_element() {
        assert_eq!(element_hash(""), 0);
        assert_eq!(element_hash("a"), 97);
        assert_eq!(element_hash("ab"), 97 * 31 + 98);
    }

    #[test]
    fn initials() {
        assert_eq!(initial("alice"), "A");
        assert_eq!(initial("@bob:example.org"), "B");
        assert_eq!(initial("#"), "#");
        assert_eq!(initial("élan"), "É");
        assert_eq!(initial(""), "");
        assert_eq!(initial("👩‍🔬 lab"), "👩‍🔬");
        assert_eq!(initial("🇩🇪 team"), "🇩🇪");
    }

    #[test]
    fn light_theme_names_are_readable() {
        let white = Color::WHITE;
        for color in PALETTE {
            let darkened = darken_to_contrast(color, white);
            assert!(contrast(darkened, white) >= MIN_CONTRAST);
            // Already readable colours stay as they are.
            if contrast(color, white) >= MIN_CONTRAST {
                assert_eq!(darkened, color);
            }
        }
    }
}
