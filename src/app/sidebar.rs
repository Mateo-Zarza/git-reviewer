//! The left panel: every repo under the root and its worktrees.

use std::path::Path;

use iced::widget::scrollable::Direction;
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Alignment::Center, Color, Element, Fill, Padding};
use iced_palace::widget::ellipsized_text;

use super::{App, Message};
use crate::git::{Repo, Workspace, Worktree};
use crate::theme::icons::{self, icon};
use crate::theme::{self, colors, fonts, icon_sizes, layouts, text_sizes};

impl App {
    pub(super) fn sidebar<'a>(&'a self, workspace: &'a Workspace) -> Element<'a, Message> {
        let selected = self.selected_worktree_path.as_deref();

        container(
            scrollable(
                column![
                    container(
                        text(format!("REPOSITORIES · {}", workspace.repos().len()))
                            .size(text_sizes::LABEL)
                            .font(fonts::UI_SEMIBOLD)
                            .color(colors::TEXT_MUTED),
                    )
                    .padding([0.0, 8.0]),
                    column(workspace.repos().iter().map(|repo| repo_view(repo, selected)))
                        .spacing(layouts::REPO_GAP),
                ]
                .spacing(layouts::REPO_GAP),
            )
            .direction(Direction::Vertical(theme::thin_scrollbar()))
            .style(theme::scrollbar),
        )
        .width(layouts::SIDEBAR_WIDTH)
        .height(Fill)
        .padding([layouts::PANEL_PADDING_Y, layouts::ROW_PADDING_X])
        .style(|_| container::background(colors::SIDEBAR))
        .into()
    }
}

fn repo_view<'a>(repo: &'a Repo, selected: Option<&Path>) -> Element<'a, Message> {
    let count = repo.worktrees().len();
    let count_label = if count == 1 {
        "1 worktree".to_string()
    } else {
        format!("{count} worktrees")
    };

    column![
        row![
            icon(icons::CHEVRON_DOWN, icon_sizes::CHEVRON, colors::TEXT_MUTED),
            text(repo.name()).font(fonts::UI_SEMIBOLD),
            text(count_label)
                .size(text_sizes::SMALL)
                .color(colors::TEXT_DIM),
        ]
        .spacing(6)
        .align_y(Center)
        .padding([2.0, 8.0]),
        column(
            repo.worktrees()
                .iter()
                .map(|wt| worktree_view(wt, selected))
        )
        .spacing(2),
    ]
    .spacing(2)
    .into()
}

fn worktree_view<'a>(worktree: &'a Worktree, selected: Option<&Path>) -> Element<'a, Message> {
    let label = worktree.branch().unwrap_or(&worktree.head()[..7]);
    let is_selected = selected == Some(worktree.path());
    let status = worktree.cached_status();
    let tag = |label: &'static str, border: Color, color: Color| {
        container(
            text(label)
                .size(text_sizes::TAG)
                .line_height(layouts::TAG_LINE_HEIGHT),
        )
        .padding(layouts::TAG_PADDING)
        .style(theme::tag(border, color))
    };

    let main_tag = worktree
        .is_main()
        .then(|| tag("main", colors::TAG_BORDER, colors::TEXT_MUTED));
    let gone_tag = status
        .is_some_and(|s| s.is_upstream_gone())
        .then(|| tag("gone", colors::DANGER_BORDER, colors::DANGER));
    let change_pill = status
        .map(|s| s.change_count())
        .filter(|count| *count > 0)
        .map(|count| {
            container(
                text(count.to_string())
                    .size(text_sizes::LABEL)
                    .font(fonts::UI_SEMIBOLD)
                    .line_height(layouts::PILL_LINE_HEIGHT),
            )
            .padding(layouts::PILL_PADDING)
            .style(theme::pill)
        });

    // Badges are pushed only when present: a `None` child in `row!` still takes a
    // spacing gap, which would eat into the label's width.
    let badges = [main_tag, gone_tag, change_pill]
        .into_iter()
        .flatten()
        .map(Element::from);

    button(
        row![
            icon(icons::GIT_BRANCH, icon_sizes::ROW, colors::TEXT_MUTED),
            ellipsized_text(label)
                .wrapping(Wrapping::None)
                .width(Fill),
        ]
        .extend(badges)
        .spacing(layouts::GAP)
        .align_y(Center),
    )
    .on_press(Message::WorktreeSelected(worktree.path().to_path_buf()))
    .width(Fill)
    .padding(Padding::from([6.0, 8.0]).left(26.0))
    .style(theme::list_row(is_selected))
    .into()
}
