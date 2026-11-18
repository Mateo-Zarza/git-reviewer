mod diff;
mod repo;
mod status;
mod workspace;
mod worktree;

pub use diff::{Diff, DiffLine, DiffSource};
pub use repo::Repo;
pub use status::{ChangeKind, FileEntry, Status};
use std::path::{Path, PathBuf};
pub use workspace::Workspace;
pub use worktree::Worktree;

#[derive(Debug)]
pub enum GitError {
    Io(std::io::Error),
    CommandFailed(String),
}

impl From<std::io::Error> for GitError {
    fn from(value: std::io::Error) -> Self {
        GitError::Io(value)
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
