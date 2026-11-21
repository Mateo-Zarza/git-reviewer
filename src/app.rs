//! The application: state, messages, `update`, and `view`, which puts the top bar over
//! the three panels. Each panel lives in its own file as an `impl App` block; being child
//! modules, they can read `App`'s private fields directly.

mod changes;
mod diff;
mod sidebar;
mod watch;

use std::{env::args, path::PathBuf};

use iced::widget::operation;
use iced::widget::scrollable::RelativeOffset;
use iced::widget::{Container, button, column, container, row, rule, space, text, text_editor};
use iced::{Alignment::Center, Color, Element, Fill, Subscription, Task};

use crate::git::{ChangeKind, Diff, DiffSource, GitError, Repo, Status, Workspace, Worktree};
use crate::theme::icons::{self, icon};
use crate::theme::{self, colors, fonts, layouts, text_sizes};
use diff::DIFF_SCROLLABLE;

#[derive(Debug, Clone)]
pub enum Message {
    WorktreeSelected(PathBuf),
    FileSelected { path: String, source: DiffSource },
    Refresh,
    /// Something changed on disk in a worktree (from the file watcher).
    FilesChanged,
    WatchFailed(String),
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
            Message::Refresh | Message::FilesChanged => {
                self.workspace = Workspace::scan(&self.root);
                self.load_status();
                self.retarget_selected_file();
            }
            Message::WatchFailed(reason) => {
                self.error = Some(GitError::CommandFailed(format!(
                    "Auto-refresh unavailable ({reason}); use Refresh"
                )));
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

    /// Watches every worktree's folder so the app refreshes itself after edits or
    /// terminal git commands.
    pub fn subscription(&self) -> Subscription<Message> {
        let Ok(workspace) = &self.workspace else {
            return Subscription::none();
        };

        let mut paths: Vec<PathBuf> = workspace
            .repos()
            .iter()
            .flat_map(|repo| repo.worktrees())
            .map(|worktree| worktree.path().to_path_buf())
            .collect();
        // Sorted so the same worktrees always hash the same, and iced keeps the
        // running watcher instead of restarting it on every refresh.
        paths.sort();

        watch::subscription(paths)
    }

    pub fn view(&self) -> Element<'_, Message> {
        let body: Element<'_, Message> = match &self.workspace {
            Err(err) => empty_state(err.to_string(), colors::DANGER)
                .center(Fill)
                .into(),
            Ok(workspace) => row![
                self.sidebar(workspace),
                rule::vertical(1).style(theme::divider),
                container(self.changes_panel())
                    .width(layouts::CHANGES_WIDTH)
                    .height(Fill)
                    .style(|_| container::background(colors::SURFACE)),
                rule::vertical(1).style(theme::divider),
                container(self.diff_panel()).width(Fill).height(Fill),
            ]
            .height(Fill)
            .into(),
        };

        column![
            self.top_bar(),
            rule::horizontal(1).style(theme::divider),
            body
        ]
        .into()
    }

    fn top_bar(&self) -> Element<'_, Message> {
        let bar_button = |icon_bytes: &'static [u8], label: &'static str, message: Message| {
            button(
                row![icon(icon_bytes, 14.0, colors::TEXT), text(label)]
                    .spacing(6)
                    .height(Fill)
                    .align_y(Center),
            )
            .on_press(message)
            .height(layouts::BUTTON_HEIGHT)
            .padding([0.0, 12.0])
            .style(theme::outline_button)
        };

        container(
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
                bar_button(icons::DOWNLOAD, "Fetch all", Message::FetchAll),
                bar_button(icons::ROTATE_CW, "Refresh", Message::Refresh),
            ]
            .spacing(12)
            .align_y(Center),
        )
        .height(layouts::TOP_BAR_HEIGHT)
        .padding([0.0, layouts::PANEL_PADDING_X])
        .align_y(Center)
        .style(|_| container::background(colors::SURFACE))
        .into()
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

/// A centered, muted message for panels with nothing to show (shared by the panels).
fn empty_state<'a>(message: impl Into<String>, color: Color) -> Container<'a, Message> {
    container(text(message.into()).color(color).center()).padding([24.0, 8.0])
}
