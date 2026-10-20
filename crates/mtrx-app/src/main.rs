//! Mtrx desktop app.

// matrix-sdk futures are deeply nested.
#![recursion_limit = "256"]

mod actions;
mod app;
mod arrange;
mod avatar;
mod calls;
mod dialog;
mod ellipsis;
mod emoji;
mod fonts;
mod hint;
mod icons;
mod info;
mod last_view;
mod login;
mod logo;
mod math;
mod media;
mod members;
mod message;
mod pdf;
mod pdf_viewer;
mod popover;
mod room;
mod send_later;
mod settings;
mod sidebar;
mod style;
mod tint;

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

    iced::application(app::App::boot, app::App::update, app::App::view)
        .title("Mtrx")
        .subscription(app::App::subscription)
        .theme(app::App::theme)
        .settings(fonts::settings())
        .window(iced::window::Settings {
            size: iced::Size::new(1200.0, 800.0),
            icon: logo::window_icon(),
            ..Default::default()
        })
        .run()
}
