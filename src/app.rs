use crate::git::{
    ChangeKind, Diff, DiffLine, FileEntry, GitError, Repo, Status, Workspace, Worktree,
};
use iced::{
    Color, Element, Fill, Font,
    widget::{button, column, container, row, scrollable, text},
};
use std::{env::args, path::PathBuf};

pub struct App {
    workspace: Result<Workspace, GitError>,
    selected_worktree: Option<PathBuf>,
    selected_file: Option<(String, bool)>,
    status: Option<Result<Status, GitError>>,
    root: PathBuf,
    diff: Option<Result<Diff, GitError>>,
}
impl App {
    pub fn new() -> Self {
        let path = args().nth(1).unwrap_or_else(|| ".".to_string());
        let workspace = Workspace::scan(&path);

        App {
            workspace,
            selected_worktree: None,
            selected_file: None,
            status: None,
            root: path.into(),
            diff: None,
        }
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::WorktreeSelected(path) => {
                self.selected_worktree = Some(path);
                self.diff = None;
                self.selected_file = None;
                self.load_status();
            }
            Message::Refresh => {
                self.workspace = Workspace::scan(&self.root);
                self.load_status();
                self.load_diff();
            }
            Message::FileSelected { path, staged } => {
                self.selected_file = Some((path, staged));
                self.load_diff();
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let body: Element<'_, Message> = match &self.workspace {
            Err(err) => text(err.to_string()).into(),
            Ok(workspace) => row![
                scrollable(
                    column(workspace.repos().iter().map(repo_view))
                        .width(260)
                        .spacing(10)
                ),
                container(status_view(&self.status)).width(280),
                scrollable(diff_view(&self.diff)).width(Fill)
            ]
            .spacing(10)
            .padding(10)
            .into(),
        };

        column![button(text("Refresh")).on_press(Message::Refresh), body].into()
    }

    fn load_status(&mut self) {
        if let (Some(selected), Ok(workspace)) = (&self.selected_worktree, &self.workspace) {
            self.status = workspace
                .find_worktree(selected)
                .map(|worktree| worktree.status());
        }
    }

    fn load_diff(&mut self) {
        if let (Some(selected), Some((file, staged)), Ok(workspace)) = (
            &self.selected_worktree,
            &self.selected_file,
            &self.workspace,
        ) {
            self.diff = workspace
                .find_worktree(selected)
                .map(|worktree| worktree.diff(file, *staged));
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    WorktreeSelected(PathBuf),
    FileSelected { path: String, staged: bool },
    Refresh,
}

fn repo_view(repo: &Repo) -> Element<'_, Message> {
    column![
        text(repo.name()),
        column(repo.worktrees().iter().map(worktree_view))
    ]
    .into()
}

fn worktree_view(worktree: &Worktree) -> Element<'_, Message> {
    let branch = worktree.branch().unwrap_or("(detached)");
    let marker = if worktree.is_main() {
        " [main checkout]"
    } else {
        ""
    };
    button(text(format!("{branch}{marker}")))
        .on_press(Message::WorktreeSelected(worktree.path().to_path_buf()))
        .into()
}

fn status_view(status: &Option<Result<Status, GitError>>) -> Element<'_, Message> {
    match status {
        None => text("Select a worktree").into(),
        Some(Err(err)) => text(err.to_string()).into(),
        Some(Ok(status)) => {
            let branch = status.branch.as_deref().unwrap_or("(detached)");
            let (ahead, behind) = status
                .upstream
                .as_ref()
                .and_then(|upstream| upstream.counts)
                .unwrap_or((0, 0));

            let staged_count = status.files.iter().filter(|f| f.staged.is_some()).count();
            let unstaged_count = status.files.iter().filter(|f| f.unstaged.is_some()).count();

            column![
                row![
                    text(format!("Branch: {branch} ")),
                    text(format!("↑{ahead} ↓{behind}")),
                ],
                text(format!("STAGED CHANGES: {staged_count}")),
                column(
                    status
                        .files
                        .iter()
                        .filter_map(|file| file.staged.map(|kind| file_view(file, kind, true)))
                ),
                text(format!("CHANGES: {unstaged_count}")),
                column(
                    status
                        .files
                        .iter()
                        .filter_map(|file| file.unstaged.map(|kind| file_view(file, kind, false)))
                ),
            ]
            .into()
        }
    }
}

fn file_view(file: &FileEntry, kind: ChangeKind, staged: bool) -> Element<'_, Message> {
    let renamed_marker = if let Some(old) = &file.old_path {
        format!("{} → ", old)
    } else {
        "".to_string()
    };

    button(text(format!(
        "{} {}{} ",
        kind.to_code(),
        renamed_marker,
        file.path,
    )))
    .on_press(Message::FileSelected {
        path: file.path.clone(),
        staged,
    })
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
    let (content, bg, fg) = match line {
        DiffLine::Hunk(content) => (
            format!("{content}"),
            Color::from_rgb8(34, 37, 44),
            Some(Color::from_rgb8(143, 166, 200)),
        ),
        DiffLine::Added(content) => (
            format!("+{content}"),
            Color::from_rgba8(124, 196, 138, 0.13),
            None,
        ),
        DiffLine::Removed(content) => (
            format!("-{content}"),
            Color::from_rgba8(240, 133, 122, 0.13),
            None,
        ),
        DiffLine::Context(content) => (format!(" {content}"), Color::TRANSPARENT, None),
    };

    container(text(content).font(Font::MONOSPACE).color_maybe(fg))
        .width(Fill)
        .style(move |_theme| container::background(bg))
        .into()
}
