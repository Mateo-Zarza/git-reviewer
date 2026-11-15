use crate::git::{FileEntry, GitError, Repo, Status, Workspace, Worktree};
use iced::{
    Element,
    widget::{button, column, row, text},
};
use std::{env::args, path::PathBuf};

pub struct App {
    workspace: Result<Workspace, GitError>,
    selected: Option<PathBuf>,
    status: Option<Result<Status, GitError>>,
}
impl App {
    pub fn new() -> Self {
        let path = args().nth(1).unwrap_or_else(|| ".".to_string());
        let workspace = Workspace::scan(path);

        App {
            workspace,
            selected: None,
            status: None,
        }
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::WorktreeSelected(path) => {
                if let Ok(workspace) = &self.workspace {
                    self.status = workspace
                        .find_worktree(&path)
                        .map(|worktree| worktree.status());
                }
                self.selected = Some(path);
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        match &self.workspace {
            Err(err) => text(err.to_string()).into(),
            Ok(workspace) => column![
                column(workspace.repos().iter().map(repo_view)),
                status_view(&self.status)
            ]
            .into(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    WorktreeSelected(PathBuf),
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

            column![
                row![
                    text(format!("Branch: {branch} ")),
                    text(format!("↑{ahead} ↓{behind}")),
                ],
                column(status.files.iter().map(|file| file_view(file)))
            ]
            .into()
        }
    }
}

fn file_view(file: &FileEntry) -> Element<'_, Message> {
    let x = file.staged.map_or('.', |s| s.to_code());
    let y = file.unstaged.map_or('.', |s| s.to_code());
    let renamed_marker = if let Some(old) = &file.old_path {
        format!("{} → ", old)
    } else {
        "".to_string()
    };

    text(format!("{}{} {}{} ", x, y, renamed_marker, file.path,)).into()
}
