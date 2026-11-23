// Release builds on Windows are GUI apps: without this, Windows opens a console window
// next to the app. Debug builds (`cargo run`) keep theirs, for `eprintln!` and panics.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use iced::{Settings, Size, window};

use crate::app::{App, min_window_width};
use crate::theme::text_sizes;

mod app;
mod config;
mod git;
mod highlight;
mod state;
mod theme;

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .title("Git Reviewer")
        .subscription(App::subscription)
        .settings(Settings {
            default_text_size: text_sizes::BODY.into(),
            ..Settings::default()
        })
        // Narrow enough to put every panel at its minimum width, and no narrower. No
        // minimum height (0).
        .window(window::Settings {
            min_size: Some(Size::new(min_window_width(), 0.0)),
            ..window::Settings::default()
        })
        // Inter: the UI font where there's no system one we target (see theme::fonts).
        .font(include_bytes!("../assets/fonts/Inter-Regular.ttf").as_slice())
        .font(include_bytes!("../assets/fonts/Inter-SemiBold.ttf").as_slice())
        .font(include_bytes!("../assets/fonts/FiraCode-Regular.ttf").as_slice())
        .theme(theme::app_theme())
        .run()
}
