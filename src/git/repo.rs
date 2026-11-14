use std::path::{PathBuf, absolute};

use crate::git::{Git, GitError, Worktree};

pub struct Repo {
    git: Git,
    name: String,
    worktrees: Vec<Worktree>,
}

impl Repo {
    pub fn open(path: impl Into<PathBuf>) -> Result<Repo, GitError> {
        let git = Git::new(absolute(path.into())?);

        let name = git
            .dir()
            .file_name()
            .map(|os_str| os_str.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unknown".into());

        let mut repo = Repo {
            git,
            name,
            worktrees: Vec::new(),
        };

        repo.refresh()?;

        Ok(repo)
    }

    pub fn refresh(&mut self) -> Result<(), GitError> {
        let cmd_output = self.git.run(&["worktree", "list", "--porcelain"])?;

        self.worktrees = Worktree::parse_list(&cmd_output);

        Ok(())
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn worktrees(&self) -> &[Worktree] {
        &self.worktrees
    }
}
