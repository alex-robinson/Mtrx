//! Mtrx desktop app.

// matrix-sdk futures are deeply nested.
#![recursion_limit = "256"]

mod app;
mod avatar;
mod ellipsis;
mod fonts;
mod icons;
mod login;
mod math;
mod message;
mod room;
mod send_later;
mod settings;
mod sidebar;
mod style;

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

    iced::application(app::App::boot, app::App::update, app::App::view)
        .title("Mtrx")
        .subscription(app::App::subscription)
        .theme(app::App::theme)
        .settings(fonts::settings())
        .window_size((1200.0, 800.0))
        .run()
}
