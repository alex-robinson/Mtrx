//! Mtrx desktop app.

// No console window next to the app on Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]
// matrix-sdk futures are deeply nested.
#![recursion_limit = "256"]

mod actions;
mod app;
mod appearance;
mod arrange;
mod avatar;
mod badge;
mod calls;
mod context_menu;
mod dialog;
mod drag_scroll;
mod ellipsis;
mod emoji;
mod find;
mod fonts;
mod hint;
mod icons;
mod info;
mod last_view;
mod link_status;
#[cfg(target_os = "linux")]
mod linux;
mod login;
mod logo;
mod math;
mod media;
mod members;
mod mention;
mod message;
mod message_menu;
mod pdf;
mod pdf_viewer;
mod placement;
mod popover;
mod presence;
mod room;
mod selectable;
mod selectable_image;
mod selection;
mod send_later;
mod settings;
mod sidebar;
mod style;
mod text_size;
mod tint;
#[cfg(windows)]
mod windows;

fn main() -> iced::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "warn,matrix_sdk=error,matrix_sdk::http_client=off,matrix_sdk::latest_events=off,\
                 wgpu=error,naga=error"
                    .into()
            }),
        )
        .init();
    #[cfg(target_os = "macos")]
    logo::notifications_from_bundle();

    let environment = app::Environment::load().expect("valid MTRX_PROFILE and data directory");
    let window = environment.window();
    iced::application(
        move || app::App::boot(environment.clone()),
        app::App::update,
        app::App::view,
    )
    .title("Mtrx")
    .subscription(app::App::subscription)
    .theme(app::App::window_theme)
    .scale_factor(app::App::scale_factor)
    .settings(fonts::settings())
    .window(iced::window::Settings {
        icon: logo::window_icon(),
        // The app sets its presence offline and ends its session first, see
        // `app::Message::CloseWindow`.
        exit_on_close_request: false,
        #[cfg(target_os = "linux")]
        platform_specific: linux::window_settings(),
        // Size, position and maximised as the window was left.
        ..window
    })
    .run()
}
