use crate::git::{
    ChangeKind, Diff, DiffLine, DiffSource, FileEntry, GitError, Repo, Status, Workspace, Worktree,
};
use iced::{
    Alignment::Center,
    Color, Element, Fill, Font, Padding,
    alignment::{Horizontal::Right, Vertical::Bottom},
    font,
    widget::{button, column, container, row, scrollable, text, text_editor},
};
use std::{env::args, path::PathBuf};

#[derive(Debug, Clone)]
pub enum Message {
    WorktreeSelected(PathBuf),
    FileSelected { path: String, source: DiffSource },
    Refresh,
    Stage(String),
    Unstage(String),
    CommitMessageEdited(text_editor::Action),
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
        }
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::WorktreeSelected(path) => {
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
                let worktree = self.selected_worktree();

                if let Some(worktree) = worktree {
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
                let worktree = self.selected_worktree();

                if let Some(worktree) = worktree {
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
                container(status_view(
                    &self.status,
                    &self.commit_message,
                    self.selected_repo().map(|r| r.name())
                ))
                .width(280),
                scrollable(diff_view(&self.diff)).width(Fill)
            ]
            .spacing(10)
            .padding(10)
            .into(),
        };

        column![
            row![
                button(text("Refresh")).on_press(Message::Refresh),
                text(
                    self.error
                        .as_ref()
                        .map(|e| e.to_string())
                        .unwrap_or_default()
                )
                .color(Color::from_rgb8(240, 133, 122))
            ],
            body
        ]
        .into()
    }

    fn load_status(&mut self) {
        self.status = self.selected_worktree().map(|worktree| worktree.status());
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

fn status_view<'a>(
    status: &'a Option<Result<Status, GitError>>,
    commit_message: &'a text_editor::Content,
    repo_name: Option<&'a str>,
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

            column![
                row![
                    text(branch).size(15).font(Font {
                        weight: font::Weight::Semibold,
                        ..Font::default()
                    }),
                    text(repo_name.unwrap_or_default())
                        .size(13)
                        .color(Color::from_rgb8(161, 163, 171)),
                    text(upstream_branch)
                        .size(12)
                        .color(Color::from_rgb8(139, 142, 151)),
                ]
                .spacing(8)
                .align_y(Bottom),
                text("COMMIT MESSAGE")
                    .size(11)
                    .color(Color::from_rgb8(161, 163, 171)),
                text_editor(commit_message)
                    .placeholder("Message (Ctrl+Enter to commit)")
                    .on_action(Message::CommitMessageEdited)
                    .height(72),
                text(format!("STAGED CHANGES · {staged_count}"))
                    .size(11)
                    .color(Color::from_rgb8(161, 163, 171)),
                column(
                    status
                        .files
                        .iter()
                        .filter_map(|file| file.staged.map(|kind| file_view(file, kind, true)))
                ),
                text(format!("CHANGES · {unstaged_count}"))
                    .size(11)
                    .color(Color::from_rgb8(161, 163, 171)),
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
                            .font(Font {
                                weight: font::Weight::Bold,
                                ..Font::MONOSPACE
                            })
                            .color(kind_color(kind))
                            .width(12)
                    )
                    .padding(Padding {
                        top: 6.0,
                        bottom: 0.0,
                        left: 0.0,
                        right: 0.0
                    }),
                    text(name),
                    text(dir).size(12).color(Color::from_rgb8(139, 142, 151))
                ]
                .width(Fill)
                .spacing(8)
                .align_y(Center),
                button_sign.width(22).style(button::text)
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
            Color::from_rgb8(34, 37, 44),
            Some(Color::from_rgb8(143, 166, 200)),
            None,
            None,
            " ",
            None,
        ),
        DiffLine::Added { new, text } => (
            text.as_str(),
            Color::from_rgba8(124, 196, 138, 0.13),
            None,
            None,
            Some(*new),
            "+",
            Some(Color::from_rgb8(124, 196, 138)),
        ),
        DiffLine::Removed { old, text } => (
            text.as_str(),
            Color::from_rgba8(240, 133, 122, 0.13),
            None,
            Some(*old),
            None,
            "-",
            Some(Color::from_rgb8(240, 133, 122)),
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
                .width(44)
                .font(Font::MONOSPACE)
                .align_x(Right),
            text(new_label)
                .width(44)
                .font(Font::MONOSPACE)
                .align_x(Right),
            text(sign).font(Font::MONOSPACE).color_maybe(sg),
            text(content).font(Font::MONOSPACE).color_maybe(fg)
        ]
        .spacing(8),
    )
    .width(Fill)
    .style(move |_theme| container::background(bg))
    .into()
}

fn kind_color(kind: ChangeKind) -> Color {
    match kind {
        ChangeKind::Added => Color::from_rgb8(124, 196, 138),
        ChangeKind::Renamed => Color::from_rgb8(111, 179, 232),
        ChangeKind::Copied => Color::from_rgb8(124, 196, 138),
        ChangeKind::Modified => Color::from_rgb8(229, 181, 103),
        ChangeKind::Deleted => Color::from_rgb8(240, 133, 122),
        ChangeKind::TypeChanged => Color::from_rgb8(229, 181, 103),
        ChangeKind::Untracked => Color::from_rgb8(124, 196, 138),
    }
}
