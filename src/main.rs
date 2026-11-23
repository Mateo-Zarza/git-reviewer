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
            icon: Some(window_icon()),
            ..window::Settings::default()
        })
        // Inter: the UI font where there's no system one we target (see theme::fonts).
        .font(include_bytes!("../assets/fonts/Inter-Regular.ttf").as_slice())
        .font(include_bytes!("../assets/fonts/Inter-SemiBold.ttf").as_slice())
        .font(include_bytes!("../assets/fonts/FiraCode-Regular.ttf").as_slice())
        .theme(theme::app_theme())
        .run()
}

/// The icon in the window's title bar and taskbar button (on Windows; macOS takes the
/// Dock icon from the .app). Drawn from the same SVG the .exe and .app icons come from.
fn window_icon() -> window::Icon {
    use resvg::{tiny_skia, usvg};

    const SIZE: u32 = 64;
    let svg = include_bytes!("../assets/app-icon/icon.svg");
    let tree = usvg::Tree::from_data(svg, &usvg::Options::default())
        .expect("the bundled app icon is valid SVG");
    let mut pixmap = tiny_skia::Pixmap::new(SIZE, SIZE).expect("the size isn't zero");
    let scale = SIZE as f32 / tree.size().width();
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    // tiny-skia stores premultiplied alpha; the window icon wants it straight.
    let rgba = pixmap
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let color = pixel.demultiply();
            [color.red(), color.green(), color.blue(), color.alpha()]
        })
        .collect();
    window::icon::from_rgba(rgba, SIZE, SIZE).expect("the pixels match the size")
}
