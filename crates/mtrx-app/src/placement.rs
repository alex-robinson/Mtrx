//! The window's size and position, and whether it is maximised, per
//! profile, so Mtrx opens where it was left (`window.json` in the profile's
//! directory). Full screen is not kept: the window opens as it was before.

use std::path::{Path, PathBuf};

use iced::{Point, Rectangle, Size, window};
use mtrx_core::Profile;
use serde::{Deserialize, Serialize};
use tracing::warn;

/// The window's size on first opening, in logical pixels.
pub const DEFAULT_SIZE: Size = Size::new(1200.0, 800.0);

/// How much of the title bar must be on a display to drag the window by:
/// its top [`TITLE_BAR`] pixels, at least [`GRIP`] of them.
const TITLE_BAR: f32 = 28.0;
const GRIP: Size = Size::new(80.0, 20.0);

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Placement {
    /// The window when last neither maximised nor full screen.
    pub frame: Option<Frame>,
    pub maximized: bool,
}

/// A window's outer top-left corner, in the system's screen coordinates, and
/// its inner size, in logical pixels as [`window::position`] gives them.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Frame {
    pub fn new(position: Point, size: Size) -> Self {
        Self {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
        }
    }

    pub fn position(&self) -> Point {
        Point::new(self.x, self.y)
    }

    pub fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }

    /// Whether the title bar is on one of `displays`, enough to drag the
    /// window by.
    pub fn reachable(&self, displays: &[Rectangle]) -> bool {
        let title_bar = Rectangle::new(self.position(), Size::new(self.width, TITLE_BAR));
        displays.iter().any(|display| {
            display
                .intersection(&title_bar)
                .is_some_and(|on| on.width >= GRIP.width && on.height >= GRIP.height)
        })
    }

    /// Where a window opens that is not on any of `displays`: the default
    /// size, centred on the main display, the first.
    pub fn fallback(displays: &[Rectangle]) -> Option<Self> {
        let main = displays.first()?;
        let size = Size::new(
            DEFAULT_SIZE.width.min(main.width),
            DEFAULT_SIZE.height.min(main.height),
        );
        let position = main.center() - iced::Vector::new(size.width, size.height) * 0.5;
        Some(Self::new(position, size))
    }
}

impl Placement {
    pub fn path(profile: &Profile) -> PathBuf {
        profile.dir().join("window.json")
    }

    /// The saved placement; none if there is none or it cannot be read.
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Also before signing in, when the profile's directory may not exist.
    pub fn save(&self, path: &Path) {
        let saved = serde_json::to_vec_pretty(self)
            .map_err(std::io::Error::other)
            .and_then(|json| {
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                std::fs::write(path, json)
            });
        if let Err(e) = saved {
            warn!("could not save the window's placement: {e}");
        }
    }

    /// The window as it was left, its size in layout units at `scale` (the
    /// text size's, which iced applies to it).
    pub fn settings(&self, scale: f32) -> window::Settings {
        let (size, position) = match self.frame {
            Some(frame) => (frame.size(), window::Position::Specific(frame.position())),
            None => (DEFAULT_SIZE, window::Position::Default),
        };
        window::Settings {
            size: size / scale,
            position,
            maximized: self.maximized,
            ..window::Settings::default()
        }
    }

    /// Follow the window to `frame`, unless it is maximised or full screen,
    /// when its frame is the screen's; whether anything changed.
    pub fn follow(&mut self, frame: Frame, maximized: bool, fullscreen: bool) -> bool {
        if fullscreen {
            return false;
        }
        let before = *self;
        self.maximized = maximized;
        if !maximized {
            self.frame = Some(frame);
        }
        *self != before
    }
}

/// The work areas of the displays attached (without menu bar, Dock or
/// taskbar), the main display first, in `window`'s logical pixels, as
/// [`window::position`] counts them; `None` where the platform does not say
/// (Linux, where Wayland places windows itself and X11 window managers keep
/// them on screen).
#[cfg(target_os = "macos")]
pub fn displays(_window: &dyn window::Window) -> Option<Vec<Rectangle>> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSScreen;

    // The runtime calls this on the main thread.
    let screens = NSScreen::screens(MainThreadMarker::new()?);
    // AppKit counts up from the main display's bottom, winit down from its
    // top.
    let main = screens.firstObject()?.frame();
    let top = main.origin.y + main.size.height;
    Some(
        screens
            .iter()
            .map(|screen| {
                let area = screen.visibleFrame();
                Rectangle {
                    x: area.origin.x as f32,
                    y: (top - area.origin.y - area.size.height) as f32,
                    width: area.size.width as f32,
                    height: area.size.height as f32,
                }
            })
            .collect(),
    )
}

#[cfg(windows)]
pub fn displays(window: &dyn window::Window) -> Option<Vec<Rectangle>> {
    crate::windows::work_areas(window)
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn displays(_window: &dyn window::Window) -> Option<Vec<Rectangle>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAPTOP: Rectangle = Rectangle {
        x: 0.0,
        y: 25.0,
        width: 1512.0,
        height: 920.0,
    };
    /// To the laptop's right.
    const MONITOR: Rectangle = Rectangle {
        x: 1512.0,
        y: 0.0,
        width: 2560.0,
        height: 1415.0,
    };

    fn frame(x: f32, y: f32) -> Frame {
        Frame::new(Point::new(x, y), Size::new(1000.0, 700.0))
    }

    #[test]
    fn saves_and_loads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("window.json");
        assert_eq!(Placement::load(&path), Placement::default());
        let placement = Placement {
            frame: Some(frame(1800.0, 200.0)),
            maximized: true,
        };
        placement.save(&path);
        assert_eq!(Placement::load(&path), placement);
    }

    #[test]
    fn opens_as_left() {
        let placement = Placement {
            frame: Some(frame(1800.0, 200.0)),
            maximized: true,
        };
        let settings = placement.settings(2.0);
        assert_eq!(settings.size, Size::new(500.0, 350.0));
        assert!(matches!(
            settings.position,
            window::Position::Specific(Point {
                x: 1800.0,
                y: 200.0
            })
        ));
        assert!(settings.maximized);

        let first = Placement::default().settings(1.0);
        assert_eq!(first.size, DEFAULT_SIZE);
        assert!(matches!(first.position, window::Position::Default));
        assert!(!first.maximized);
    }

    /// Maximised and full screen, the window's frame is the screen's: the
    /// one before is kept, to go back to.
    #[test]
    fn keeps_the_frame_before_maximising() {
        let mut placement = Placement::default();
        assert!(placement.follow(frame(100.0, 100.0), false, false));
        assert!(!placement.follow(frame(100.0, 100.0), false, false));
        assert!(placement.follow(frame(0.0, 25.0), true, false));
        assert_eq!(placement.frame, Some(frame(100.0, 100.0)));
        assert!(placement.maximized);
        assert!(!placement.follow(frame(0.0, 0.0), false, true));
        assert_eq!(placement.frame, Some(frame(100.0, 100.0)));
        assert!(placement.maximized);
        assert!(placement.follow(frame(200.0, 100.0), false, false));
        assert_eq!(placement.frame, Some(frame(200.0, 100.0)));
        assert!(!placement.maximized);
    }

    /// A window left on a display since unplugged is not opened off screen.
    #[test]
    fn reachable_on_a_display() {
        let both = [LAPTOP, MONITOR];
        assert!(frame(100.0, 100.0).reachable(&[LAPTOP]));
        assert!(frame(1800.0, 200.0).reachable(&both));
        assert!(!frame(1800.0, 200.0).reachable(&[LAPTOP]));
        // Mostly off the left edge, the title bar's right end still showing.
        assert!(frame(-900.0, 100.0).reachable(&[LAPTOP]));
        assert!(!frame(-960.0, 100.0).reachable(&[LAPTOP]));
        // Below the bottom, or the title bar above the top.
        assert!(!frame(100.0, 940.0).reachable(&[LAPTOP]));
        assert!(!frame(100.0, -20.0).reachable(&[LAPTOP]));

        let fallback = Frame::fallback(&[LAPTOP, MONITOR]).unwrap();
        assert_eq!(fallback.size(), DEFAULT_SIZE);
        assert_eq!(fallback.position(), Point::new(156.0, 85.0));
        assert!(fallback.reachable(&[LAPTOP]));
        assert_eq!(Frame::fallback(&[]), None);
    }
}
