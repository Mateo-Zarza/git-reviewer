mod worktree;

use std::path::{Path, PathBuf};

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
