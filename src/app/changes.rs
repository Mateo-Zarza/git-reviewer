//! The middle panel: header, commit box, staged/unstaged file list and the removal footer.

use iced::keyboard::{Key, key::Named};
use iced::widget::scrollable::Direction;
use iced::widget::text::Wrapping;
use iced::widget::text_editor::{Binding, KeyPress};
use iced::widget::{
    Button, Row, button, column, container, hover, row, rule, scrollable, space, text,
    text_editor,
};
use iced::{
    Alignment::Center,
    Border, Color, Element, Fill, Padding,
    alignment::{Horizontal, Vertical::Bottom},
};
use iced_palace::widget::ellipsized_text;

use std::f32::consts::TAU;

use iced::{Radians, Rotation};

use super::file_icon::file_icon;
use super::{App, DiscardScope, Message, RemoteOp, Removal, empty_state};
use crate::git::{ChangeKind, DiffSource, FileEntry, Status};
use crate::theme::icons::{self, icon};
use crate::theme::{self, colors, fonts, icon_sizes, layouts, text_sizes};

impl App {
    pub(super) fn changes_panel(&self) -> Element<'_, Message> {
        let status = match &self.status {
            None => {
                return empty_state("Select a worktree", colors::TEXT_MUTED)
                    .center(Fill)
                    .into();
            }
            Some(Err(err)) => {
                return empty_state(err.to_string(), colors::DANGER)
                    .center(Fill)
                    .into();
            }
            Some(Ok(status)) => status,
        };

        column![
            self.changes_header(status),
            rule::horizontal(1).style(theme::divider),
            self.commit_section(status),
            rule::horizontal(1).style(theme::divider),
            scrollable(self.file_list(status))
                .direction(Direction::Vertical(theme::thin_scrollbar()))
                .style(theme::scrollbar)
                .height(Fill),
            rule::horizontal(1).style(theme::divider),
            container(self.changes_footer(status))
                .padding([layouts::PANEL_PADDING_Y, layouts::PANEL_PADDING_X]),
        ]
        .into()
    }

    fn changes_header<'a>(&'a self, status: &'a Status) -> Element<'a, Message> {
        let branch = status.branch.as_deref().unwrap_or("(detached)");
        let upstream = status.upstream.as_ref().map(|u| u.name.as_str());
        let repo_name = self.selected_repo().map(|repo| repo.name());
        // Only shown when the base isn't the upstream itself, or it would repeat Sync's ↑↓.
        let base_line = self
            .base_counts
            .as_ref()
            .filter(|(base, _)| Some(base.as_str()) != upstream)
            .map(|(base, (ahead, behind))| format!("↑{ahead} ↓{behind} from {base}"));

        column![
            row![
                ellipsized_text(branch)
                    .size(text_sizes::BRANCH)
                    .font(fonts::UI_SEMIBOLD)
                    .wrapping(Wrapping::None),
                ellipsized_text(repo_name.unwrap_or_default())
                    .size(text_sizes::BRANCH)
                    .color(colors::TEXT_MUTED)
                    .wrapping(Wrapping::None),
                ellipsized_text(upstream.unwrap_or_default())
                    .size(text_sizes::SMALL)
                    .color(colors::TEXT_DIM)
                    .wrapping(Wrapping::None),
            ]
            .spacing(layouts::GAP)
            .align_y(Bottom),
            // A broken base from the config takes the base line's place, in red.
            match &self.base_error {
                Some(problem) => Some(
                    text(problem.as_str())
                        .size(text_sizes::LABEL)
                        .color(colors::DANGER)
                ),
                None => base_line.map(|line| {
                    text(line)
                        .size(text_sizes::LABEL)
                        .color(colors::TEXT_DIM)
                }),
            },
        ]
        .padding(Padding::from([layouts::PANEL_PADDING_Y, layouts::PANEL_PADDING_X]).top(14.0))
        .into()
    }

    fn commit_section<'a>(&'a self, status: &'a Status) -> Element<'a, Message> {
        let staged_count = status.files.iter().filter(|f| f.staged.is_some()).count();
        let unstaged_count = status.files.iter().filter(|f| f.unstaged.is_some()).count();
        let counts = status.upstream.as_ref().and_then(|u| u.counts);
        let can_commit = staged_count > 0 && !self.commit_message.is_empty();

        // What the primary button shows and does, VS Code-style: Commit while there are
        // changes, then Sync while ahead/behind the upstream, Publish if there's none.
        // While a Sync/Publish runs it stays on that action, disabled, icon spinning.
        let (icon_bytes, title, counts_text, on_press) = match self.remote_op {
            Some((RemoteOp::Sync, _)) => (Some(icons::SYNC), "Sync changes", None, None),
            Some((RemoteOp::Publish, _)) => (Some(icons::SYNC), "Publish", None, None),
            None => match (staged_count, unstaged_count, counts) {
                (1.., _, _) => (
                    Some(icons::CHECK),
                    "Commit",
                    None,
                    can_commit.then_some(Message::Commit),
                ),
                (0, 0, Some((ahead, behind))) if ahead + behind > 0 => (
                    Some(icons::SYNC),
                    "Sync changes",
                    Some(format!("↑{ahead} ↓{behind}")),
                    Some(Message::Sync),
                ),
                (0, 0, None) if status.branch.is_some() => {
                    (Some(icons::UPLOAD), "Publish", None, Some(Message::Publish))
                }
                (0, 1.., _) => (Some(icons::CHECK), "Commit", None, None),
                _ => (None, "No changes", None, None),
            },
        };

        // The icon isn't text, so the button style's text color doesn't reach it; it gets
        // the same color as the label by hand (see `theme::primary_button`).
        let icon_color = if on_press.is_some() {
            colors::ON_PRIMARY
        } else {
            colors::TEXT_DIM
        };
        // One turn per second since the operation started; 0 when idle.
        let angle = self
            .remote_op
            .map(|(_, started)| started.elapsed().as_secs_f32() * TAU)
            .unwrap_or(0.0);

        // Pushed piece by piece: a `None` child in `row!` would still take a gap.
        let mut content = row![].spacing(layouts::GAP).align_y(Center);
        if let Some(bytes) = icon_bytes {
            content = content.push(
                icon(bytes, icon_sizes::BUTTON, icon_color)
                    .rotation(Rotation::Floating(Radians(angle))),
            );
        }
        content = content.push(text(title).font(fonts::UI_SEMIBOLD));
        if let Some(counts_text) = counts_text {
            content = content.push(
                text(counts_text)
                    .font(fonts::UI_SEMIBOLD)
                    .size(text_sizes::SMALL),
            );
        }

        container(
            column![
                text("COMMIT MESSAGE")
                    .size(text_sizes::LABEL)
                    .color(colors::TEXT_MUTED)
                    .font(fonts::UI_SEMIBOLD),
                text_editor(&self.commit_message)
                    .placeholder(if cfg!(target_os = "macos") {
                        "Message (⌘+Enter to commit)"
                    } else {
                        "Message (Ctrl+Enter to commit)"
                    })
                    .on_action(Message::CommitMessageEdited)
                    .key_binding(move |key_press| commit_shortcut(key_press, can_commit))
                    .padding([8.0, 10.0])
                    .height(layouts::COMMIT_BOX_HEIGHT)
                    .style(theme::commit_input),
                button(container(content).center(Fill))
                    .on_press_maybe(on_press)
                    .height(layouts::PRIMARY_BUTTON_HEIGHT)
                    .width(Fill)
                    .style(theme::primary_button),
            ]
            .spacing(layouts::GAP),
        )
        .padding([layouts::PANEL_PADDING_Y, layouts::PANEL_PADDING_X])
        .into()
    }

    fn file_list<'a>(&'a self, status: &'a Status) -> Element<'a, Message> {
        if status.files.is_empty() {
            return empty_state("No changes in this worktree", colors::TEXT_MUTED)
                .center_x(Fill)
                .into();
        }

        // One group per side of the index; like VS Code, an empty group isn't shown.
        let group = |title: &str, staged: bool| -> Option<Element<'a, Message>> {
            let rows: Vec<Element<'a, Message>> = status
                .files
                .iter()
                .filter_map(|file| {
                    let kind = if staged { file.staged } else { file.unstaged }?;
                    Some(self.file_row(file, kind, staged))
                })
                .collect();

            (!rows.is_empty()).then(|| {
                column![
                    section_header(format!("{title} · {}", rows.len()), staged),
                    column(rows).spacing(1),
                ]
                .into()
            })
        };

        // `flatten` drops hidden groups, so a lone group gets no stray SECTION_GAP.
        let groups = [group("STAGED CHANGES", true), group("CHANGES", false)]
            .into_iter()
            .flatten();

        container(column(groups).spacing(layouts::SECTION_GAP))
            .padding(layouts::GAP)
            .into()
    }

    fn file_row<'a>(
        &'a self,
        file: &'a FileEntry,
        kind: ChangeKind,
        staged: bool,
    ) -> Element<'a, Message> {
        let source = if staged {
            DiffSource::Staged
        } else if kind == ChangeKind::Untracked {
            DiffSource::Untracked
        } else {
            DiffSource::Unstaged
        };
        let is_selected = self
            .selected_file
            .as_ref()
            .is_some_and(|(path, selected_source)| *path == file.path && *selected_source == source);
        // VS Code's order: discard first, then stage; a staged file can only be unstaged.
        let actions = if staged {
            row![icon_button(
                icons::MINUS,
                Message::Unstage(file.path.clone()),
                is_selected,
            )]
        } else {
            let discard = DiscardScope::File {
                path: file.path.clone(),
                untracked: kind == ChangeKind::Untracked,
            };
            row![
                icon_button(icons::DISCARD, Message::DiscardRequested(discard), is_selected),
                icon_button(icons::PLUS, Message::Stage(file.path.clone()), is_selected),
            ]
        };
        let (dir, name) = file.path.rsplit_once('/').unwrap_or(("", &file.path));

        // The name gets the space first and only it is cut ("…") if it doesn't fit; the
        // folder gets what's left, cut too, and disappears when nothing is left (the
        // clip hides the lone "..." ellipsized text would draw at zero width).
        let name_and_dir = row![
            ellipsized_text(name).wrapping(Wrapping::None),
            container(
                ellipsized_text(dir)
                    .size(text_sizes::SMALL)
                    .color(colors::TEXT_DIM)
                    .wrapping(Wrapping::None),
            )
            .width(Fill)
            .clip(true),
        ]
        .spacing(layouts::GAP)
        .align_y(Center);

        // Icon and status letter have fixed widths, so iced sizes them first and they
        // always keep their place; the name and folder share what's left in between.
        let row_button = button(
            row![
                file_icon(&file.path),
                container(name_and_dir).width(Fill),
                text(kind.to_code().to_string())
                    .font(fonts::UI_SEMIBOLD)
                    .color(kind_color(kind))
                    .width(STATUS_LETTER_WIDTH)
                    .align_x(Horizontal::Center),
            ]
            .spacing(layouts::GAP)
            .height(Fill)
            .align_y(Center),
        )
        .on_press(Message::FileSelected {
            path: file.path.clone(),
            source,
        })
        .width(Fill)
        .height(layouts::LIST_ROW_HEIGHT)
        .padding([0.0, 8.0])
        .style(theme::list_row(is_selected));

        // While hovered the row is HOVER (or SELECTED, which wins); the actions sit on a
        // patch of that same color so the name slides under them, like VS Code.
        let row_color = if is_selected {
            colors::SELECTED
        } else {
            colors::HOVER
        };

        // The buttons stop just short of the status letter, which stays visible, like
        // VS Code: row padding + letter + a small gap from the right edge.
        let actions_inset = 8.0 + STATUS_LETTER_WIDTH + 4.0;
        hover(row_button, hover_actions(actions, row_color, actions_inset))
    }

    fn changes_footer<'a>(&'a self, status: &'a Status) -> Element<'a, Message> {
        let worktree_removable = !self.selected_worktree().is_some_and(|w| w.is_main());
        // Deliberate deviation from VS Code: only offer to delete branches with nothing
        // unmerged into the base (no base known → let `git branch -d` decide).
        // A configured base that doesn't resolve can't vouch for anything: fail closed.
        let branch_deletable = worktree_removable
            && status.branch.is_some()
            && self.base_error.is_none()
            && self
                .base_counts
                .as_ref()
                .is_none_or(|(_, (ahead, _))| *ahead == 0);
        // With uncommitted changes, `update` asks in a dialog before removing anything.
        let footer_button = |label: &'static str, removal: Removal, enabled: bool| {
            button(text(label).width(Fill).height(Fill).center())
                .on_press_maybe(enabled.then_some(Message::RemovalRequested(removal)))
                .height(layouts::BUTTON_HEIGHT)
                .width(Fill)
                .style(theme::danger_button)
        };

        row![
            footer_button("Rm. worktree", Removal::Worktree, worktree_removable),
            footer_button("Del. branch", Removal::Branch, branch_deletable),
        ]
        .spacing(layouts::GAP)
        .into()
    }
}

/// The commit box's key handling: Ctrl+Enter (Cmd+Enter on macOS, which is what
/// `modifiers.command()` means) commits; every other key behaves as usual.
/// iced calls this for every key press, focused or not, hence the focus check.
fn commit_shortcut(key_press: KeyPress, can_commit: bool) -> Option<Binding<Message>> {
    let is_shortcut = matches!(key_press.status, text_editor::Status::Focused { .. })
        && key_press.modifiers.command()
        && key_press.key == Key::Named(Named::Enter);

    if is_shortcut {
        // When committing isn't possible yet, swallow the shortcut rather than let it
        // insert a newline, like VS Code.
        return can_commit.then_some(Binding::Custom(Message::Commit));
    }

    Binding::from_key_press(key_press)
}

/// "STAGED CHANGES · 3" with its stage-all / unstage-all button on the right.
fn section_header<'a>(title: String, staged: bool) -> Element<'a, Message> {
    let actions = if staged {
        row![icon_button(icons::MINUS, Message::UnstageAll, false)]
    } else {
        row![
            icon_button(icons::DISCARD, Message::DiscardRequested(DiscardScope::All), false),
            icon_button(icons::PLUS, Message::StageAll, false),
        ]
    };

    let title = container(
        text(title)
            .size(text_sizes::LABEL)
            .font(fonts::UI_SEMIBOLD)
            .color(colors::TEXT_MUTED),
    )
    .width(Fill)
    .height(layouts::LIST_ROW_HEIGHT)
    .padding([0.0, 8.0])
    .align_y(Center);

    hover(title, hover_actions(actions, colors::SURFACE, 4.0))
}

/// The status letter column at the right end of a file row (wide enough for a
/// semibold "M" at the body size). Letters are centered in it: they differ in width
/// (M is wider than D or A), so right-aligned they started at different places and the
/// column looked ragged; centered, they share one center line.
const STATUS_LETTER_WIDTH: f32 = 14.0;

/// The buttons a row shows only while hovered, right-aligned over the row, `inset`
/// from its right edge. They sit on a patch of `background` (the hovered row's color)
/// so text underneath doesn't show through; clicks anywhere else fall through to the
/// row itself.
fn hover_actions<'a>(
    buttons: Row<'a, Message>,
    background: Color,
    inset: f32,
) -> Element<'a, Message> {
    row![
        space().width(Fill),
        container(buttons.spacing(4))
            .padding(Padding::ZERO.left(4.0))
            .style(move |_| container::Style {
                background: Some(background.into()),
                border: Border {
                    radius: theme::radii::CONTROL.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            }),
    ]
    .height(Fill)
    .align_y(Center)
    .padding(Padding::ZERO.right(inset))
    .into()
}

/// A small square button holding a row-size icon (24px around 15px). `on_selected_row`
/// picks a hover color that still shows on the selected row's blue.
fn icon_button<'a>(
    bytes: &'static [u8],
    message: Message,
    on_selected_row: bool,
) -> Button<'a, Message> {
    // Pinned at a whole-pixel offset rather than centered: centering 15px in 24px puts
    // the icon at 4.5px, a half pixel, which smears its thin strokes across two pixel
    // columns (the + looked dim). Half a pixel off-center isn't visible; blur is.
    let before = ((layouts::ICON_BUTTON - icon_sizes::ROW) / 2.0).floor();
    let after = layouts::ICON_BUTTON - icon_sizes::ROW - before;

    button(icon(bytes, icon_sizes::ROW, colors::TEXT_MUTED))
        .on_press(message)
        .width(layouts::ICON_BUTTON)
        .height(layouts::ICON_BUTTON)
        .padding(Padding {
            top: before,
            left: before,
            right: after,
            bottom: after,
        })
        .style(theme::icon_button(on_selected_row))
}

fn kind_color(kind: ChangeKind) -> Color {
    match kind {
        ChangeKind::Added => colors::ADDED,
        ChangeKind::Renamed => colors::RENAMED,
        ChangeKind::Copied => colors::ADDED,
        ChangeKind::Modified => colors::MODIFIED,
        ChangeKind::Deleted => colors::DANGER,
        ChangeKind::TypeChanged => colors::MODIFIED,
        ChangeKind::Untracked => colors::ADDED,
    }
}
