use std::path::{Path, PathBuf, absolute};

use crate::git::{Git, GitError, Worktree};

#[derive(Clone)]
pub struct Repo {
    git: Git,
    name: String,
    worktrees: Vec<Worktree>,
}
impl Repo {
    /// Opens the repo and lists its worktrees, each with its `git status` loaded.
    pub fn open(path: impl Into<PathBuf>) -> Result<Repo, GitError> {
        let mut repo = Repo::open_without_status(path)?;
        repo.load_statuses();

        Ok(repo)
    }

    /// Opens the repo and lists its worktrees, but runs no `git status`: their cached
    /// status stays `None`. One git call instead of one per worktree, for repos that
    /// are only listed (the archived ones).
    pub fn open_without_status(path: impl Into<PathBuf>) -> Result<Repo, GitError> {
        let git = Git::new(absolute(path.into())?);

        let name = git
            .dir()
            .file_name()
            .map(|os_str| os_str.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unknown".into());

        let worktrees = Worktree::parse_list(&git.run(&["worktree", "list", "--porcelain"])?);

        Ok(Repo {
            git,
            name,
            worktrees,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// The repo's folder (its main worktree), absolute.
    pub fn path(&self) -> &Path {
        self.git.dir()
    }

    pub fn worktrees(&self) -> &[Worktree] {
        &self.worktrees
    }

    pub(super) fn worktrees_mut(&mut self) -> &mut [Worktree] {
        &mut self.worktrees
    }

    fn load_statuses(&mut self) {
        // One `git status` per worktree, run in parallel so a repo with many
        // worktrees doesn't multiply the refresh time.
        std::thread::scope(|scope| {
            for worktree in &mut self.worktrees {
                scope.spawn(move || worktree.refresh_cached_status());
            }
        });
    }

    pub fn fetch(&self) -> Result<(), GitError> {
        self.git.run(&["fetch", "--prune"])?;

        Ok(())
    }

    /// Fetches every repo in parallel and returns the first error, if any.
    /// One call for all repos, so the app rescans once when everything is done.
    pub fn fetch_all(repos: &[Repo]) -> Result<(), GitError> {
        std::thread::scope(|scope| {
            let handles: Vec<_> = repos
                .iter()
                .map(|repo| scope.spawn(|| repo.fetch()))
                .collect();

            handles
                .into_iter()
                .try_for_each(|handle| handle.join().expect("fetch thread panicked"))
        })
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
