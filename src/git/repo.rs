use std::path::{Path, PathBuf, absolute};

use crate::git::{Git, GitError, Worktree};

#[derive(Clone)]
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

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn worktrees(&self) -> &[Worktree] {
        &self.worktrees
    }

    pub fn refresh(&mut self) -> Result<(), GitError> {
        let cmd_output = self.git.run(&["worktree", "list", "--porcelain"])?;

        self.worktrees = Worktree::parse_list(&cmd_output);

        Ok(())
    }

    pub fn fetch(&self) -> Result<(), GitError> {
        self.git.run(&["fetch", "--prune"])?;

        Ok(())
    }

    pub fn default_base(&self) -> Option<String> {
        self.git
            .run(&["symbolic-ref", "--short", "refs/remotes/origin/HEAD"])
            .ok()
            .map(|s| s.trim().to_string())
    }

    pub fn remove_worktree(&self, path: &Path, force: bool) -> Result<(), GitError> {
        let path = path.to_string_lossy();
        let mut args = vec!["worktree", "remove"];
        if force {
            args.push("--force");
        }
        args.push(&path);
        self.git.run(&args)?;

        Ok(())
    }

    pub fn delete_branch(&self, name: &str) -> Result<(), GitError> {
        self.git.run(&["branch", "-d", name])?;

        Ok(())
    }
}
