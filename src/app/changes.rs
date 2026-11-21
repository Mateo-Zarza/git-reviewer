//! The middle panel: header, commit box, staged/unstaged file list and the removal footer.

use iced::keyboard::{Key, key::Named};
use iced::widget::scrollable::Direction;
use iced::widget::text::Wrapping;
use iced::widget::text_editor::{Binding, KeyPress};
use iced::widget::{Button, button, column, container, row, rule, scrollable, text, text_editor};
use iced::{
    Alignment::Center, Color, Element, Fill, Padding, alignment::Vertical::Bottom,
};
use iced_palace::widget::ellipsized_text;

use super::{App, Message, Removal, empty_state};
use crate::git::{ChangeKind, DiffSource, FileEntry, Status};
use crate::theme::icons::{self, icon};
use crate::theme::{self, colors, fonts, layouts, text_sizes};

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
        let label = |label: &'static str| -> Element<'a, Message> {
            text(label).font(fonts::UI_SEMIBOLD).into()
        };

        // What the primary button says and does, VS Code-style: Commit while there are
        // changes, then Sync while ahead/behind the upstream, Publish if there's none.
        let (content, on_press): (Element<'a, Message>, Option<Message>) =
            match (staged_count, unstaged_count, counts) {
                (1.., _, _) => (label("Commit"), can_commit.then_some(Message::Commit)),
                (0, 0, Some((ahead, behind))) if ahead + behind > 0 => (
                    row![
                        label("Sync changes"),
                        text(format!("↑{ahead} ↓{behind}"))
                            .font(fonts::MONO_MEDIUM)
                            .size(text_sizes::SMALL),
                    ]
                    .spacing(10)
                    .align_y(Center)
                    .into(),
                    Some(Message::Sync),
                ),
                (0, 0, None) if status.branch.is_some() => {
                    (label("Publish"), Some(Message::Publish))
                }
                (0, 1.., _) => (label("Commit"), None),
                _ => (label("No changes"), None),
            };

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
                    .height(72)
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
        let (sign_icon, sign_message) = if staged {
            (icons::MINUS, Message::Unstage(file.path.clone()))
        } else {
            (icons::PLUS, Message::Stage(file.path.clone()))
        };
        let (dir, name) = file.path.rsplit_once('/').unwrap_or(("", &file.path));

        button(
            row![
                text(kind.to_code().to_string())
                    .font(fonts::MONO_MEDIUM)
                    .color(kind_color(kind))
                    .width(12),
                ellipsized_text(name).wrapping(Wrapping::None),
                ellipsized_text(dir)
                    .size(text_sizes::SMALL)
                    .color(colors::TEXT_DIM)
                    .wrapping(Wrapping::None)
                    .width(Fill),
                icon_button(sign_icon, sign_message),
            ]
            .spacing(layouts::GAP)
            .align_y(Center),
        )
        .on_press(Message::FileSelected {
            path: file.path.clone(),
            source,
        })
        .width(Fill)
        .padding(Padding::from([4.0, 8.0]).right(4.0))
        .style(theme::list_row(is_selected))
        .into()
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
        let dirty = !status.files.is_empty();
        let request = |removal: Removal| {
            if dirty {
                Message::ConfirmRemoval(removal)
            } else {
                removal.message(false)
            }
        };

        let footer_button = |label: &'static str| {
            button(text(label).width(Fill).height(Fill).center())
                .height(layouts::BUTTON_HEIGHT)
                .width(Fill)
        };

        match self.pending_removal {
            None => row![
                footer_button("Rm. worktree")
                    .on_press_maybe(worktree_removable.then_some(request(Removal::Worktree)))
                    .style(theme::danger_button),
                footer_button("Del. branch")
                    .on_press_maybe(branch_deletable.then_some(request(Removal::Branch)))
                    .style(theme::danger_button),
            ]
            .spacing(layouts::GAP)
            .into(),
            Some(removal) => column![
                text(format!("Discard {} changes and remove?", status.files.len()))
                    .width(Fill)
                    .center(),
                row![
                    footer_button("Remove")
                        .on_press(removal.message(true))
                        .style(theme::danger_button),
                    footer_button("Cancel")
                        .on_press(Message::CancelRemoval)
                        .style(theme::outline_button),
                ]
                .spacing(layouts::GAP),
            ]
            .spacing(layouts::GAP)
            .into(),
        }
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
    let (bytes, message) = if staged {
        (icons::MINUS, Message::UnstageAll)
    } else {
        (icons::PLUS, Message::StageAll)
    };

    row![
        text(title)
            .size(text_sizes::LABEL)
            .font(fonts::UI_SEMIBOLD)
            .color(colors::TEXT_MUTED)
            .width(Fill),
        icon_button(bytes, message),
    ]
    .align_y(Center)
    .padding(Padding::from(4.0).left(8.0))
    .into()
}

/// A 22px square button holding a 13px icon, as in the mockup's file list.
fn icon_button<'a>(bytes: &'static [u8], message: Message) -> Button<'a, Message> {
    button(container(icon(bytes, 13.0, colors::TEXT_MUTED)).center(Fill))
        .on_press(message)
        .width(layouts::ICON_BUTTON)
        .height(layouts::ICON_BUTTON)
        .padding(0)
        .style(theme::icon_button)
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
