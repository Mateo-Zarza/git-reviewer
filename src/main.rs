use crate::app::App;

mod app;
mod git;

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .title("Git Reviewer")
        .run()
}
