//! Platform specific settings for macOS.

/// The platform specific window settings of an application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlatformSpecific {
    /// Hides the window title.
    pub title_hidden: bool,
    /// Makes the titlebar transparent and allows the content to appear behind it.
    pub titlebar_transparent: bool,
    /// Makes the window content appear behind the titlebar.
    pub fullsize_content_view: bool,
    // Mtrx patch (hide on close): our winit's.
    /// Closing the window hides it, as a Mac app's main window, instead of
    /// requesting to close it; clicking the app's Dock icon shows it again.
    /// Quitting still requests to close it.
    pub hides_on_close: bool,
}
