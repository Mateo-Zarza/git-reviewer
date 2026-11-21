use crate::git::{
    ChangeKind, Diff, DiffLine, DiffSource, FileEntry, GitError, Repo, Status, Workspace, Worktree,
};
use crate::theme::icons::{self, icon};
use crate::theme::{self, colors, fonts, layouts, text_sizes};
use iced::Padding;
use iced::widget::operation;
use iced::widget::scrollable::{Direction, RelativeOffset};
use iced::widget::text::{IntoFragment, Wrapping};
use iced::widget::{Text, responsive, rule, space};
use std::borrow::Cow;
use iced_palace::widget::ellipsized_text;
use iced::{
    Alignment::Center,
    Color, Element, Fill, Pixels, Task,
    alignment::{Horizontal::Right, Vertical::Bottom},
    widget::{Button, Container, button, column, container, row, scrollable, text, text_editor},
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
    StageAll,
    UnstageAll,
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
                self.retarget_selected_file();
            }
            Message::FileSelected { path, source } => {
                self.selected_file = Some((path, source));
                self.load_diff();
                // iced keeps a scrollable's offset across content changes, so a newly
                // opened file would start wherever the previous one was scrolled to.
                return operation::snap_to(DIFF_SCROLLABLE, RelativeOffset::START);
            }
            Message::Stage(path) => {
                if let Some(worktree) = self.selected_worktree() {
                    self.error = worktree.stage(&path).err();
                }

                self.load_status();
                self.retarget_selected_file();
            }
            Message::Unstage(path) => {
                if let Some(worktree) = self.selected_worktree() {
                    self.error = worktree.unstage(&path).err();
                }

                self.load_status();
                self.retarget_selected_file();
            }
            Message::StageAll => {
                if let Some(worktree) = self.selected_worktree() {
                    self.error = worktree.stage_all().err();
                }

                self.load_status();
                self.retarget_selected_file();
            }
            Message::UnstageAll => {
                if let Some(worktree) = self.selected_worktree() {
                    self.error = worktree.unstage_all().err();
                }

                self.load_status();
                self.retarget_selected_file();
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
                    self.retarget_selected_file();
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
                )
                .direction(Direction::Vertical(theme::thin_scrollbar()))
                .style(theme::scrollbar))
                .width(layouts::SIDEBAR_WIDTH)
                .height(Fill)
                .padding([layouts::PANEL_PADDING_Y, layouts::ROW_PADDING_X])
                .style(|_| container::background(colors::SIDEBAR));

                let changes_panel = container(self.changes_panel())
                .width(layouts::CHANGES_WIDTH)
                .height(Fill)
                .style(|_| container::background(colors::SURFACE));

                let diff_panel = container(self.diff_panel()).width(Fill).height(Fill);

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

    /// After anything that changes the index (stage, unstage, commit, refresh), keeps the
    /// selected file on the side it was showing if that side still exists, otherwise
    /// moves it to its other side, or clears the selection once the file has no changes.
    /// Deriving this from the new status (instead of from the action) means a failed git
    /// command leaves the selection alone.
    fn retarget_selected_file(&mut self) {
        // No readable status (worktree gone, git failing): nothing to show a diff of.
        let Some(Ok(status)) = &self.status else {
            self.selected_file = None;
            self.diff = None;
            return;
        };
        let Some((path, source)) = &self.selected_file else {
            return;
        };

        let entry = status.files.iter().find(|file| file.path == *path);
        let staged = entry
            .and_then(|file| file.staged)
            .map(|_| DiffSource::Staged);
        let unstaged = entry.and_then(|file| file.unstaged).map(|kind| match kind {
            ChangeKind::Untracked => DiffSource::Untracked,
            _ => DiffSource::Unstaged,
        });
        let new_source = match source {
            DiffSource::Staged => staged.or(unstaged),
            DiffSource::Unstaged | DiffSource::Untracked => unstaged.or(staged),
        };

        match new_source {
            Some(new_source) => {
                self.selected_file = Some((path.clone(), new_source));
                self.load_diff();
            }
            None => {
                self.selected_file = None;
                self.diff = None;
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

    // Badges are pushed only when present: a `None` child in `row!` still takes a
    // spacing gap, which would eat into the label's width.
    let badges = [main_tag, gone_tag, change_pill]
        .into_iter()
        .flatten()
        .map(Element::from);

    button(
        row![
            icon(icons::GIT_BRANCH, 13.0, colors::TEXT_MUTED),
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

impl App {
    /// The middle panel: header, commit box, file list and the removal footer.
    fn changes_panel(&self) -> Element<'_, Message> {
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
            base_line.map(|line| {
                text(line)
                    .size(text_sizes::LABEL)
                    .color(colors::TEXT_DIM)
            }),
        ]
        .padding(Padding::from([layouts::PANEL_PADDING_Y, layouts::PANEL_PADDING_X]).top(14.0))
        .into()
    }

    fn commit_section<'a>(&'a self, status: &'a Status) -> Element<'a, Message> {
        let staged_count = status.files.iter().filter(|f| f.staged.is_some()).count();
        let unstaged_count = status.files.iter().filter(|f| f.unstaged.is_some()).count();
        let counts = status.upstream.as_ref().and_then(|u| u.counts);
        let label = |label: &'static str| -> Element<'a, Message> {
            text(label).font(fonts::UI_SEMIBOLD).into()
        };

        // What the primary button says and does, VS Code-style: Commit while there are
        // changes, then Sync while ahead/behind the upstream, Publish if there's none.
        let (content, on_press): (Element<'a, Message>, Option<Message>) =
            match (staged_count, unstaged_count, counts) {
                (1.., _, _) => (
                    label("Commit"),
                    (!self.commit_message.is_empty()).then_some(Message::Commit),
                ),
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
                (0, 0, None) if status.branch.is_some() => (label("Publish"), Some(Message::Publish)),
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
                    .placeholder("Message (Ctrl+Enter to commit)")
                    .on_action(Message::CommitMessageEdited)
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

    fn file_row<'a>(&'a self, file: &'a FileEntry, kind: ChangeKind, staged: bool) -> Element<'a, Message> {
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
        let branch_deletable = worktree_removable
            && status.branch.is_some()
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

/// A centered, muted message for panels with nothing to show.
fn empty_state<'a>(message: impl Into<String>, color: Color) -> Container<'a, Message> {
    container(text(message.into()).color(color).center()).padding([24.0, 8.0])
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

// Diff geometry, from the mockup: two 44px line-number columns padded 8 and 10 on the
// right, a 16px sign column, 24px after the code; hunk headers are padded 16 each side.
const OLD_NUMBER_PADDING: f32 = 8.0;
const NEW_NUMBER_PADDING: f32 = 10.0;
const GUTTER_WIDTH: f32 = layouts::LINE_NUMBER_WIDTH * 2.0
    + OLD_NUMBER_PADDING
    + NEW_NUMBER_PADDING
    + layouts::DIFF_SIGN_WIDTH;
const CODE_PADDING_RIGHT: f32 = 24.0;
const HUNK_PADDING_X: f32 = 16.0;
/// Lets `update` reset the diff's scroll position when another file is opened.
const DIFF_SCROLLABLE: &str = "diff";

impl App {
    /// The right panel: a header bar (path, which side, +N −N) over the scrollable diff.
    fn diff_panel(&self) -> Element<'_, Message> {
        let (Some((path, source)), Some(diff)) = (&self.selected_file, &self.diff) else {
            let clean = matches!(&self.status, Some(Ok(status)) if status.files.is_empty());
            return match (&self.status, clean) {
                (None, _) => space().into(),
                (Some(_), true) => empty_state(
                    "Nothing to review. Everything is committed.",
                    colors::TEXT_MUTED,
                )
                .center(Fill)
                .into(),
                (Some(_), false) => {
                    empty_state("Select a file to see its changes", colors::TEXT_MUTED)
                        .center(Fill)
                        .into()
                }
            };
        };

        let diff = match diff {
            Ok(diff) => diff,
            Err(err) => {
                return empty_state(err.to_string(), colors::DANGER)
                    .center(Fill)
                    .into();
            }
        };

        column![
            diff_header(path, *source, diff),
            rule::horizontal(1).style(theme::divider),
            diff_body(diff),
        ]
        .into()
    }
}

fn diff_header<'a>(path: &'a str, source: DiffSource, diff: &Diff) -> Element<'a, Message> {
    let (added, removed) = diff.counts();
    let side = match source {
        DiffSource::Staged => "Staged",
        DiffSource::Unstaged => "Unstaged",
        DiffSource::Untracked => "Untracked",
    };

    container(
        row![
            ellipsized_text(path)
                .font(fonts::MONO)
                .size(text_sizes::SMALL)
                .wrapping(Wrapping::None),
            container(text(side).size(text_sizes::LABEL))
                .padding([1.0, 6.0])
                .style(theme::filled_tag),
            space().width(Fill),
            text(format!("+{added}"))
                .font(fonts::MONO)
                .size(text_sizes::SMALL)
                .color(colors::ADDED),
            text(format!("−{removed}"))
                .font(fonts::MONO)
                .size(text_sizes::SMALL)
                .color(colors::DANGER),
        ]
        .spacing(10)
        .align_y(Center),
    )
    .height(layouts::DIFF_HEADER_HEIGHT)
    .padding([0.0, layouts::PANEL_PADDING_X])
    .align_y(Center)
    .into()
}

/// The diff lines, scrollable both ways. Lines don't wrap, so every row is given the same
/// width: the longest line's, or the panel's if that's wider, so the tints run edge to
/// edge. (Rows can't just be `Fill`: inside a both-ways scrollable that lays out empty.)
fn diff_body(diff: &Diff) -> Element<'_, Message> {
    let content_width = diff
        .lines
        .iter()
        .map(|line| {
            let chars = expand_tabs(line.text()).chars().count() as f32;
            let code = chars * layouts::DIFF_CHAR_WIDTH;
            match line {
                DiffLine::Hunk(_) => code + HUNK_PADDING_X * 2.0,
                _ => GUTTER_WIDTH + code + CODE_PADDING_RIGHT,
            }
        })
        .fold(0.0, f32::max);

    responsive(move |size| {
        let width = content_width.max(size.width);

        scrollable(
            column(diff.lines.iter().map(|line| diff_line_view(line, width))).padding([8.0, 0.0]),
        )
        .id(DIFF_SCROLLABLE)
        .direction(Direction::Both {
            vertical: theme::thin_scrollbar(),
            horizontal: theme::thin_scrollbar(),
        })
        .style(theme::scrollbar)
        .width(Fill)
        .height(Fill)
        .into()
    })
    .into()
}

fn diff_line_view(line: &DiffLine, width: f32) -> Element<'_, Message> {
    let (old, new, sign, gutter_bg, line_bg, sign_color) = match line {
        DiffLine::Hunk(header) => {
            return container(
                container(mono(expand_tabs(header)).color(colors::HUNK_TEXT))
                    .padding([2.0, HUNK_PADDING_X])
                    .width(width)
                    .style(|_| container::background(colors::HUNK_BG)),
            )
            .padding(Padding::ZERO.top(6.0).bottom(2.0))
            .into();
        }
        DiffLine::Context { old, new, .. } => (
            Some(*old),
            Some(*new),
            " ",
            colors::BACKGROUND,
            Color::TRANSPARENT,
            colors::LINE_NUMBER,
        ),
        DiffLine::Added { new, .. } => (
            None,
            Some(*new),
            "+",
            colors::ADDED_GUTTER,
            colors::ADDED_TINT,
            colors::ADDED,
        ),
        DiffLine::Removed { old, .. } => (
            Some(*old),
            None,
            "−",
            colors::REMOVED_GUTTER,
            colors::REMOVED_TINT,
            colors::DELETED,
        ),
    };

    let number = |number: Option<usize>, padding_right: f32| {
        container(
            mono(number.map(|n| n.to_string()).unwrap_or_default())
                .color(colors::LINE_NUMBER)
                .width(layouts::LINE_NUMBER_WIDTH)
                .align_x(Right),
        )
        .padding(Padding::ZERO.right(padding_right))
    };

    container(row![
        container(row![
            number(old, OLD_NUMBER_PADDING),
            number(new, NEW_NUMBER_PADDING),
            mono(sign)
                .color(sign_color)
                .width(layouts::DIFF_SIGN_WIDTH),
        ])
        .style(move |_| container::background(gutter_bg)),
        mono(expand_tabs(line.text())),
    ])
    .width(width)
    .style(move |_| container::background(line_bg))
    .into()
}

/// Diff text: JetBrains Mono 12 on a fixed 20px line, never wrapped.
fn mono<'a>(content: impl IntoFragment<'a>) -> Text<'a> {
    text(content)
        .font(fonts::MONO)
        .size(text_sizes::SMALL)
        .line_height(Pixels(layouts::DIFF_LINE_HEIGHT))
        .wrapping(Wrapping::None)
}

/// Tabs as 4 spaces (VS Code's default), so they render predictably and the width math
/// above (one glyph = one column) holds.
fn expand_tabs(line: &str) -> Cow<'_, str> {
    if line.contains('\t') {
        Cow::Owned(line.replace('\t', "    "))
    } else {
        Cow::Borrowed(line)
    }
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
