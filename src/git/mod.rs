mod diff;
mod repo;
mod status;
mod workspace;
mod worktree;

pub use diff::{Diff, DiffLine, DiffSource};
pub use repo::Repo;
pub use status::{ChangeKind, FileEntry, Status};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
pub use workspace::Workspace;
pub use worktree::Worktree;

#[derive(Debug, Clone)]
pub enum GitError {
    Io(Arc<std::io::Error>),
    CommandFailed(String),
}

impl From<std::io::Error> for GitError {
    fn from(value: std::io::Error) -> Self {
        GitError::Io(Arc::new(value))
    }
}

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GitError::Io(err) => write!(f, "IO error: {}", err),
            GitError::CommandFailed(msg) => write!(f, "Git command failed: {}", msg),
        }
    }
}

#[derive(Clone)]
struct Git {
    dir: PathBuf,
}
impl Git {
    fn new(dir: impl Into<PathBuf>) -> Self {
        Git { dir: dir.into() }
    }

    fn dir(&self) -> &Path {
        &self.dir
    }

    fn run(&self, args: &[&str]) -> Result<String, GitError> {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(self.dir())
            // Every path we pass is an exact file name from `git status`, never a
            // pattern. Without this, `restore -- [id].tsx` would also match `i.tsx` and
            // `d.tsx` (glob), and a name starting with `:` would be read as magic.
            .env("GIT_LITERAL_PATHSPECS", "1")
            .output()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
            Ok(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            Err(GitError::CommandFailed(stderr.trim().to_string()))
        }
    }
}
