use crate::git::{
    ChangeKind, Diff, DiffLine, DiffSource, FileEntry, GitError, Repo, Status, Workspace, Worktree,
};
use crate::theme::icons::{self, icon};
use crate::theme::{self, colors, fonts, layouts, text_sizes};
use iced::Padding;
use iced::widget::text::Wrapping;
use iced::widget::{rule, space};
use iced_palace::widget::ellipsized_text;
use iced::{
    Alignment::Center,
    Color, Element, Fill, Task,
    alignment::{Horizontal::Right, Vertical::Bottom},
    widget::{button, column, container, row, scrollable, text, text_editor},
};
use std::path::Path;
use std::{env::args, path::PathBuf};

#[derive(Debug, Clone)]
pub enum Message {
    WorktreeSelected(PathBuf),
    FileSelected { path: String, source: DiffSource },
    Refresh,
    Stage(String),
    Unstage(String),
    CommitMessageEdited(text_editor::Action),
    Commit,
    FetchAll,
    Fetched(Result<(), GitError>),
    Sync,
    Synced(Result<(), GitError>),
    Publish,
    Published(Result<(), GitError>),
    RemoveWorktree { force: bool },
    DeleteBranch { force: bool },
    ConfirmRemoval(Removal),
    CancelRemoval,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Removal {
    Worktree,
    Branch,
}
impl Removal {
    fn message(self, force: bool) -> Message {
        match self {
            Removal::Worktree => Message::RemoveWorktree { force },
            Removal::Branch => Message::DeleteBranch { force },
        }
    }
}

pub struct App {
    workspace: Result<Workspace, GitError>,
    selected_worktree_path: Option<PathBuf>,
    selected_file: Option<(String, DiffSource)>,
    status: Option<Result<Status, GitError>>,
    root: PathBuf,
    diff: Option<Result<Diff, GitError>>,
    error: Option<GitError>,
    commit_message: text_editor::Content,
    base_counts: Option<(String, (usize, usize))>,
    pending_removal: Option<Removal>,
}
impl App {
    pub fn new() -> Self {
        let path = args().nth(1).unwrap_or_else(|| ".".to_string());
        let workspace = Workspace::scan(&path);

        App {
            workspace,
            selected_worktree_path: None,
            selected_file: None,
            status: None,
            root: path.into(),
            diff: None,
            error: None,
            commit_message: text_editor::Content::new(),
            base_counts: None,
            pending_removal: None,
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::WorktreeSelected(path) => {
                self.pending_removal = None;
                self.selected_worktree_path = Some(path);
                self.diff = None;
                self.selected_file = None;
                self.load_status();
            }
            Message::Refresh => {
                self.workspace = Workspace::scan(&self.root);
                self.load_status();
                self.load_diff();
            }
            Message::FileSelected { path, source } => {
                self.selected_file = Some((path, source));
                self.load_diff();
            }
            Message::Stage(path) => {
                if let Some(worktree) = self.selected_worktree() {
                    self.error = worktree.stage(&path).err();
                }

                self.load_status();

                if let Some((selected_path, source)) = &mut self.selected_file {
                    if *selected_path == path {
                        *source = DiffSource::Staged;
                    }
                    self.load_diff();
                }
            }
            Message::Unstage(path) => {
                if let Some(worktree) = self.selected_worktree() {
                    self.error = worktree.unstage(&path).err();
                }

                self.load_status();

                if let (Some((selected_path, source)), Some(Ok(status))) =
                    (self.selected_file.as_mut(), self.status.as_ref())
                {
                    if *selected_path == path {
                        let unstaged_kind = status
                            .files
                            .iter()
                            .find(|file| file.path == path)
                            .and_then(|file| file.unstaged);

                        match unstaged_kind {
                            Some(ChangeKind::Untracked) => *source = DiffSource::Untracked,
                            _ => *source = DiffSource::Unstaged,
                        }
                        self.load_diff();
                    }
                }
            }
            Message::CommitMessageEdited(action) => {
                self.commit_message.perform(action);
            }
            Message::Commit => {
                if let Some(worktree) = self.selected_worktree() {
                    match worktree.commit(&self.commit_message.text()) {
                        Ok(_) => {
                            self.commit_message = text_editor::Content::new();
                            self.error = None;
                        }
                        Err(err) => {
                            self.error = Some(err);
                        }
                    }
                    self.load_status();
                }
            }
            Message::FetchAll => {
                self.error = None;
                if let Ok(workspace) = &self.workspace {
                    let repos = workspace.repos().to_vec();
                    return Task::perform(
                        async move { Repo::fetch_all(&repos) },
                        Message::Fetched,
                    );
                }
            }
            Message::Fetched(result) => {
                if let Err(err) = result {
                    self.error = Some(err);
                }
                // `fetch --prune` can make upstreams gone in any worktree, so rescan
                // everything rather than just the selected status.
                self.workspace = Workspace::scan(&self.root);
                self.load_status();
            }
            Message::Sync => {
                self.error = None;
                if let Some(worktree) = self.selected_worktree().cloned() {
                    return Task::perform(async move { worktree.sync() }, Message::Synced);
                }
            }
            Message::Synced(result) => {
                self.error = result.err();
                self.load_status();
                self.load_diff();
            }
            Message::Publish => {
                self.error = None;
                if let Some(worktree) = self.selected_worktree().cloned() {
                    return Task::perform(async move { worktree.publish() }, Message::Published);
                }
            }
            Message::Published(result) => {
                self.error = result.err();
                self.load_status();
                self.load_diff();
            }
            Message::RemoveWorktree { force } => {
                self.pending_removal = None;
                if let (Some(repo), Some(path)) =
                    (self.selected_repo(), &self.selected_worktree_path)
                {
                    match repo.remove_worktree(path, force) {
                        Ok(()) => {
                            self.error = None;
                            self.selected_worktree_path = None;
                            self.selected_file = None;
                            self.diff = None;
                        }
                        Err(err) => {
                            self.error = Some(err);
                        }
                    }
                }
                self.workspace = Workspace::scan(&self.root);
                self.load_status();
            }
            Message::DeleteBranch { force } => {
                self.pending_removal = None;
                if let (Some(repo), Some(branch), Some(path)) = (
                    self.selected_repo(),
                    self.selected_worktree().and_then(|w| w.branch()),
                    &self.selected_worktree_path,
                ) {
                    match repo
                        .remove_worktree(path, force)
                        .and_then(|_| repo.delete_branch(branch))
                    {
                        Ok(()) => {
                            self.error = None;
                        }
                        Err(err) => {
                            self.error = Some(err);
                        }
                    }
                    self.selected_worktree_path = None;
                    self.selected_file = None;
                    self.diff = None;
                }
                self.workspace = Workspace::scan(&self.root);
                self.load_status();
            }
            Message::ConfirmRemoval(removal) => {
                self.pending_removal = Some(removal);
            }
            Message::CancelRemoval => {
                self.pending_removal = None;
            }
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let top_bar = container(
            row![
                text("Git Reviewer")
                    .font(fonts::UI_SEMIBOLD)
                    .size(text_sizes::TITLE),
                text(self.root.to_string_lossy())
                    .font(fonts::MONO)
                    .size(text_sizes::SMALL)
                    .color(colors::TEXT_MUTED),
                text(
                    self.error
                        .as_ref()
                        .map(|e| e.to_string())
                        .unwrap_or_default()
                )
                .color(colors::DANGER),
                space().width(Fill),
                button(
                    row![icon(icons::DOWNLOAD, 14.0, colors::TEXT), text("Fetch all")]
                        .spacing(6)
                        .height(Fill)
                        .align_y(Center)
                )
                .on_press(Message::FetchAll)
                .height(layouts::BUTTON_HEIGHT)
                .padding([0.0, 12.0])
                .style(theme::outline_button),
                button(
                    row![icon(icons::ROTATE_CW, 14.0, colors::TEXT), text("Refresh")]
                        .spacing(6)
                        .height(Fill)
                        .align_y(Center)
                )
                .on_press(Message::Refresh)
                .height(layouts::BUTTON_HEIGHT)
                .padding([0.0, 12.0])
                .style(theme::outline_button),
            ]
            .spacing(12)
            .align_y(Center),
        )
        .height(layouts::TOP_BAR_HEIGHT)
        .padding([0.0, layouts::PANEL_PADDING_X])
        .align_y(Center)
        .style(|_| container::background(colors::SURFACE));

        let body: Element<'_, Message> = match &self.workspace {
            Err(err) => text(err.to_string()).into(),
            Ok(workspace) => {
                let sidebar = container(scrollable(
                    column![
                        container(
                            text(format!("REPOSITORIES · {}", workspace.repos().len()))
                                .size(text_sizes::LABEL)
                                .font(fonts::UI_SEMIBOLD)
                                .color(colors::TEXT_MUTED),
                        )
                        .padding([0.0, 8.0]),
                        column(workspace.repos().iter().map(|repo| {
                            repo_view(repo, self.selected_worktree_path.as_deref())
                        }))
                        .spacing(layouts::REPO_GAP),
                    ]
                    .spacing(layouts::REPO_GAP),
                ))
                .width(layouts::SIDEBAR_WIDTH)
                .height(Fill)
                .padding([layouts::PANEL_PADDING_Y, layouts::ROW_PADDING_X])
                .style(|_| container::background(colors::SIDEBAR));

                let changes_panel = container(status_view(
                    &self.status,
                    &self.commit_message,
                    self.selected_repo().map(|r| r.name()),
                    &self.base_counts,
                    self.selected_worktree().is_some_and(|w| w.is_main()),
                    self.pending_removal,
                ))
                .width(layouts::CHANGES_WIDTH)
                .height(Fill)
                .style(|_| container::background(colors::SURFACE));

                let diff_panel = scrollable(diff_view(&self.diff)).width(Fill);

                row![
                    sidebar,
                    rule::vertical(1).style(theme::divider),
                    changes_panel,
                    rule::vertical(1).style(theme::divider),
                    diff_panel
                ]
                .height(Fill)
                .into()
            }
        };

        column![top_bar, rule::horizontal(1).style(theme::divider), body].into()
    }

    fn load_status(&mut self) {
        self.status = self.selected_worktree().map(|worktree| worktree.status());

        // Keep the selected row's sidebar badges in sync with what was just loaded,
        // so stage/unstage/commit update the pill without a full rescan.
        if let (Some(status), Some(path), Ok(workspace)) = (
            &self.status,
            &self.selected_worktree_path,
            &mut self.workspace,
        ) && let Some(worktree) = workspace.find_worktree_mut(path)
        {
            worktree.set_cached_status(status.as_ref().ok().cloned());
        }

        let base = self.selected_repo().and_then(|repo| repo.default_base());

        match (base, self.selected_worktree()) {
            (Some(base), Some(worktree)) => {
                self.base_counts = worktree
                    .ahead_behind(&base)
                    .ok()
                    .map(|counts| (base, counts));
            }
            _ => {
                self.base_counts = None;
            }
        }
    }

    fn load_diff(&mut self) {
        if let (Some(selected), Some((file, source))) =
            (self.selected_worktree(), &self.selected_file)
        {
            self.diff = Some(selected.diff(&file, *source));
        }
    }

    fn selected_worktree(&self) -> Option<&Worktree> {
        match (&self.selected_worktree_path, &self.workspace) {
            (Some(selected), Ok(workspace)) => workspace.find_worktree(selected),
            _ => None,
        }
    }

    fn selected_repo(&self) -> Option<&Repo> {
        match (&self.selected_worktree_path, &self.workspace) {
            (Some(selected), Ok(workspace)) => workspace.find_repo(selected),
            _ => None,
        }
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
            icon(icons::CHEVRON_DOWN, 12.0, colors::TEXT_MUTED),
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
        container(text(label).size(text_sizes::TAG))
            .padding([1.0, 5.0])
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
                    .font(fonts::UI_SEMIBOLD),
            )
            .padding([1.0, 6.0])
            .style(theme::pill)
        });

    button(
        row![
            icon(icons::GIT_BRANCH, 13.0, colors::TEXT_MUTED),
            ellipsized_text(label)
                .wrapping(Wrapping::None)
                .width(Fill),
            main_tag,
            gone_tag,
            change_pill,
        ]
        .spacing(layouts::GAP)
        .align_y(Center),
    )
    .on_press(Message::WorktreeSelected(worktree.path().to_path_buf()))
    .width(Fill)
    .padding(Padding::from([6.0, 8.0]).left(26.0))
    .style(theme::list_row(is_selected))
    .into()
}

fn status_view<'a>(
    status: &'a Option<Result<Status, GitError>>,
    commit_message: &'a text_editor::Content,
    repo_name: Option<&'a str>,
    base_counts: &'a Option<(String, (usize, usize))>,
    is_main_worktree: bool,
    pending_removal: Option<Removal>,
) -> Element<'a, Message> {
    match status {
        None => text("Select a worktree").into(),
        Some(Err(err)) => text(err.to_string()).into(),
        Some(Ok(status)) => {
            let branch = status.branch.as_deref().unwrap_or("(detached)");
            let upstream_branch = status
                .upstream
                .as_ref()
                .map(|u| u.name.as_str())
                .unwrap_or_default();
            let staged_count = status.files.iter().filter(|f| f.staged.is_some()).count();
            let unstaged_count = status.files.iter().filter(|f| f.unstaged.is_some()).count();
            let upstream_name = status.upstream.as_ref().map(|u| u.name.as_str());
            let ahead_behind_text = base_counts
                .as_ref()
                .filter(|(base, _)| Some(base.as_str()) != upstream_name)
                .map(|(base, (ahead, behind))| format!("↑{ahead} ↓{behind} from {base}"))
                .unwrap_or_default();
            let counts = status.upstream.as_ref().and_then(|u| u.counts);
            let primary_label = |label: String| {
                text(label)
                    .width(Fill)
                    .height(Fill)
                    .center()
                    .font(fonts::UI_SEMIBOLD)
            };
            let action_button = match (staged_count, unstaged_count, counts) {
                (1.., _, _) => button(primary_label("Commit".into()))
                    .on_press_maybe((!commit_message.is_empty()).then_some(Message::Commit)),
                (0, 0, Some((ahead, behind))) if ahead + behind > 0 => {
                    button(primary_label(format!("Sync changes ↑{ahead} ↓{behind}")))
                        .on_press(Message::Sync)
                }
                (0, 0, None) if status.branch.is_some() => {
                    button(primary_label("Publish".into())).on_press(Message::Publish)
                }
                (0, 1.., _) => button(primary_label("Commit".into())),
                _ => button(primary_label("No changes".into())),
            };
            let worktree_removable = !is_main_worktree;
            let branch_deletable = worktree_removable
                && status.branch.is_some()
                && base_counts
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

            let header: Element<'_, Message> = column![
                row![
                    ellipsized_text(branch)
                        .size(text_sizes::BRANCH)
                        .font(fonts::UI_SEMIBOLD)
                        .wrapping(Wrapping::None),
                    ellipsized_text(repo_name.unwrap_or_default())
                        .size(text_sizes::BRANCH)
                        .color(colors::TEXT_MUTED)
                        .wrapping(Wrapping::None),
                    ellipsized_text(upstream_branch)
                        .size(text_sizes::SMALL)
                        .color(colors::TEXT_DIM)
                        .wrapping(Wrapping::None),
                ]
                .spacing(layouts::GAP)
                .align_y(Bottom),
                (!ahead_behind_text.is_empty()).then(|| {
                    text(ahead_behind_text)
                        .size(text_sizes::LABEL)
                        .color(colors::TEXT_DIM)
                }),
            ]
            .padding(Padding::from([layouts::PANEL_PADDING_Y, layouts::PANEL_PADDING_X]).top(14.0))
            .into();

            let commit_section: Element<'_, Message> = container(
                column![
                    text("COMMIT MESSAGE")
                        .size(text_sizes::LABEL)
                        .color(colors::TEXT_MUTED)
                        .font(fonts::UI_SEMIBOLD),
                    text_editor(commit_message)
                        .placeholder("Message (Ctrl+Enter to commit)")
                        .on_action(Message::CommitMessageEdited)
                        .height(72),
                    action_button
                        .style(theme::primary_button)
                        .height(layouts::PRIMARY_BUTTON_HEIGHT)
                        .width(Fill),
                ]
                .spacing(layouts::GAP),
            )
            .padding([layouts::PANEL_PADDING_Y, layouts::PANEL_PADDING_X])
            .into();

            let file_list: Element<'_, Message> = container(
                column![
                    column![
                        text(format!("STAGED CHANGES · {staged_count}"))
                            .size(text_sizes::LABEL)
                            .color(colors::TEXT_MUTED),
                        column(status.files.iter().filter_map(|file| {
                            file.staged.map(|kind| file_view(file, kind, true))
                        })),
                    ],
                    column![
                        text(format!("CHANGES · {unstaged_count}"))
                            .size(text_sizes::LABEL)
                            .color(colors::TEXT_MUTED),
                        column(status.files.iter().filter_map(|file| {
                            file.unstaged.map(|kind| file_view(file, kind, false))
                        })),
                    ]
                ]
                .spacing(layouts::SECTION_GAP),
            )
            .padding(layouts::GAP)
            .into();

            let footer: Element<'_, Message> = match pending_removal {
                None => row![
                    button(text("Rm. worktree").width(Fill).height(Fill).center())
                        .on_press_maybe(worktree_removable.then_some(request(Removal::Worktree)))
                        .height(layouts::BUTTON_HEIGHT)
                        .width(Fill)
                        .style(theme::danger_button),
                    button(text("Del. branch").width(Fill).height(Fill).center())
                        .on_press_maybe(branch_deletable.then_some(request(Removal::Branch)))
                        .height(layouts::BUTTON_HEIGHT)
                        .width(Fill)
                        .style(theme::danger_button),
                ]
                .spacing(layouts::GAP)
                .into(),
                Some(removal) => column![
                    text(format!(
                        "Discard {} changes and remove?",
                        status.files.len()
                    ))
                    .width(Fill)
                    .center(),
                    row![
                        button(text("Remove").width(Fill).height(Fill).center())
                            .on_press(removal.message(true))
                            .style(theme::danger_button)
                            .height(layouts::BUTTON_HEIGHT)
                            .width(Fill),
                        button(text("Cancel").width(Fill).height(Fill).center())
                            .on_press(Message::CancelRemoval)
                            .style(theme::outline_button)
                            .height(layouts::BUTTON_HEIGHT)
                            .width(Fill),
                    ]
                    .spacing(layouts::GAP)
                ]
                .spacing(layouts::GAP)
                .into(),
            };

            column![
                header,
                rule::horizontal(1).style(theme::divider),
                commit_section,
                rule::horizontal(1).style(theme::divider),
                scrollable(file_list).height(Fill),
                rule::horizontal(1).style(theme::divider),
                container(footer).padding([layouts::PANEL_PADDING_Y, layouts::PANEL_PADDING_X])
            ]
            .into()
        }
    }
}

fn file_view(file: &FileEntry, kind: ChangeKind, staged: bool) -> Element<'_, Message> {
    let source = if staged {
        DiffSource::Staged
    } else if kind == ChangeKind::Untracked {
        DiffSource::Untracked
    } else {
        DiffSource::Unstaged
    };

    let button_sign = if staged {
        button(text("-")).on_press(Message::Unstage(file.path.clone()))
    } else {
        button(text("+")).on_press(Message::Stage(file.path.clone()))
    };

    let (dir, name) = file.path.rsplit_once('/').unwrap_or(("", &file.path));

    row![
        button(
            row![
                row![
                    container(
                        text(kind.to_code().to_string())
                            .font(fonts::MONO_MEDIUM)
                            .color(kind_color(kind))
                            .width(12)
                    ),
                    text(name),
                    text(dir).size(text_sizes::SMALL).color(colors::TEXT_DIM)
                ]
                .width(Fill)
                .spacing(8)
                .align_y(Center),
                button_sign.width(layouts::GAP).style(button::text)
            ]
            .align_y(Center)
        )
        .style(button::text)
        .on_press(Message::FileSelected {
            path: file.path.clone(),
            source,
        }),
    ]
    .into()
}

fn diff_view(diff: &Option<Result<Diff, GitError>>) -> Element<'_, Message> {
    match diff {
        None => text("Select a file").into(),
        Some(Err(err)) => text(err.to_string()).into(),
        Some(Ok(diff)) => column(diff.lines.iter().map(diff_line_view)).into(),
    }
}

fn diff_line_view(line: &DiffLine) -> Element<'_, Message> {
    let (content, bg, fg, old, new, sign, sg) = match line {
        DiffLine::Hunk(text) => (
            text.as_str(),
            colors::HUNK_BG,
            Some(colors::HUNK_TEXT),
            None,
            None,
            " ",
            None,
        ),
        DiffLine::Added { new, text } => (
            text.as_str(),
            colors::ADDED_TINT,
            None,
            None,
            Some(*new),
            "+",
            Some(colors::ADDED),
        ),
        DiffLine::Removed { old, text } => (
            text.as_str(),
            colors::REMOVED_TINT,
            None,
            Some(*old),
            None,
            "-",
            Some(colors::DELETED),
        ),
        DiffLine::Context { old, new, text } => (
            text.as_str(),
            Color::TRANSPARENT,
            None,
            Some(*old),
            Some(*new),
            " ",
            None,
        ),
    };

    let old_label = old.map(|n| n.to_string()).unwrap_or_default();
    let new_label = new.map(|n| n.to_string()).unwrap_or_default();

    container(
        row![
            text(old_label)
                .width(layouts::LINE_NUMBER_WIDTH)
                .font(fonts::MONO)
                .align_x(Right),
            text(new_label)
                .width(layouts::LINE_NUMBER_WIDTH)
                .font(fonts::MONO)
                .align_x(Right),
            text(sign).font(fonts::MONO).color_maybe(sg),
            text(content).font(fonts::MONO).color_maybe(fg)
        ]
        .spacing(8),
    )
    .width(Fill)
    .style(move |_theme| container::background(bg))
    .into()
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
