use iced::Settings;

use crate::{app::App, theme::text_sizes};

mod app;
mod git;
mod theme;

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .title("Git Reviewer")
        .settings(Settings {
            default_text_size: text_sizes::BODY.into(),
            ..Settings::default()
        })
        .font(include_bytes!("../assets/fonts/Inter-Regular.ttf").as_slice())
        .font(include_bytes!("../assets/fonts/Inter-Medium.ttf").as_slice())
        .font(include_bytes!("../assets/fonts/Inter-SemiBold.ttf").as_slice())
        .font(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf").as_slice())
        .font(include_bytes!("../assets/fonts/JetBrainsMono-Medium.ttf").as_slice())
        .theme(theme::app_theme())
        .run()
}
