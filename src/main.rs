use iced::Settings;

use crate::{app::App, theme::text_sizes};

mod app;
mod config;
mod git;
mod theme;

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .title("Git Reviewer")
        .subscription(App::subscription)
        .settings(Settings {
            default_text_size: text_sizes::BODY.into(),
            ..Settings::default()
        })
        // Inter: the UI font where there's no system one we target (see theme::fonts).
        .font(include_bytes!("../assets/fonts/Inter-Regular.ttf").as_slice())
        .font(include_bytes!("../assets/fonts/Inter-SemiBold.ttf").as_slice())
        .font(include_bytes!("../assets/fonts/FiraCode-Regular.ttf").as_slice())
        .theme(theme::app_theme())
        .run()
}
