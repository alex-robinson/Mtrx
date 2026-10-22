//! Light or dark: chosen, or as the system is.
//!
//! The system's appearance comes from iced: [`iced::system::theme`] at start
//! and [`iced::system::theme_changes`] after, which winit reports for a
//! window whose appearance is not fixed by the app:
//!
//! - macOS: a window's `effectiveAppearance`, which follows the app's
//!   (`NSApp.appearance`) and, while that is unset, the system's. A chosen
//!   appearance is therefore set on the app ([`Appearance::apply`]), not
//!   on the window: winit ignores changes of a window whose own appearance
//!   is set, so iced would stop hearing of the system's.
//! - Windows: the system's setting, on `WM_SETTINGCHANGE`; iced fixes the
//!   window's title bar to a chosen appearance.
//! - Linux: the desktop portal's colour scheme (iced's
//!   `linux-theme-detection`), which GNOME and KDE provide; without a portal
//!   the app is light.

use std::fmt;

use iced::{Theme, theme::Mode};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Appearance {
    Light,
    Dark,
    #[default]
    System,
}

impl Appearance {
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];

    /// The theme the app is drawn in, with `system` the mode iced reports.
    pub fn theme(self, system: Mode) -> Theme {
        let dark = match self {
            Self::Light => false,
            Self::Dark => true,
            Self::System => system == Mode::Dark,
        };
        if dark { Theme::Dark } else { Theme::Light }
    }

    /// The theme iced fixes the window to; `None` leaves the window to
    /// follow the system, so that iced hears of its changes. On macOS the
    /// window follows the app's appearance, set by [`Self::apply`].
    pub fn window_theme(self) -> Option<Theme> {
        if cfg!(target_os = "macos") || self == Self::System {
            None
        } else {
            Some(self.theme(Mode::None))
        }
    }

    /// Sets the app's appearance, which its windows follow: the chosen one,
    /// or the system's. AppKit only on the main thread, where iced boots and
    /// updates the app (not in tests).
    #[cfg(target_os = "macos")]
    pub fn apply(self) {
        use objc2::MainThreadMarker;
        use objc2_app_kit::{
            NSAppearance, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSApplication,
        };

        let Some(main_thread) = MainThreadMarker::new() else {
            return;
        };
        // SAFETY: AppKit's constant appearance names.
        let name = match self {
            Self::Light => Some(unsafe { NSAppearanceNameAqua }),
            Self::Dark => Some(unsafe { NSAppearanceNameDarkAqua }),
            Self::System => None,
        };
        let appearance = name.and_then(NSAppearance::appearanceNamed);
        NSApplication::sharedApplication(main_thread).setAppearance(appearance.as_deref());
    }
}

impl fmt::Display for Appearance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::System => "Follow system",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_from_choice_and_system() {
        for system in [Mode::None, Mode::Light, Mode::Dark] {
            assert_eq!(Appearance::Light.theme(system), Theme::Light);
            assert_eq!(Appearance::Dark.theme(system), Theme::Dark);
        }
        assert_eq!(Appearance::System.theme(Mode::Dark), Theme::Dark);
        assert_eq!(Appearance::System.theme(Mode::Light), Theme::Light);
        assert_eq!(Appearance::System.theme(Mode::None), Theme::Light);
        assert_eq!(Appearance::default(), Appearance::System);
    }

    #[test]
    fn window_follows_the_system_unless_fixed() {
        assert_eq!(Appearance::System.window_theme(), None);
        let fixed = |a: Appearance| a.window_theme();
        if cfg!(target_os = "macos") {
            assert_eq!(fixed(Appearance::Light), None, "set on the app");
            assert_eq!(fixed(Appearance::Dark), None, "set on the app");
        } else {
            assert_eq!(fixed(Appearance::Light), Some(Theme::Light));
            assert_eq!(fixed(Appearance::Dark), Some(Theme::Dark));
        }
    }
}
