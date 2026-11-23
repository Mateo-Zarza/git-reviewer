use std::{
    fs::read_dir,
    path::{Path, PathBuf, absolute},
};

use crate::git::{GitError, Repo, Worktree};

#[derive(Clone)]
pub struct Workspace {
    repos: Vec<Repo>,
    /// Repos set aside by the user: their worktrees are listed but their statuses aren't
    /// loaded or kept up to date, and the `find_*` lookups never return them.
    archived: Vec<Repo>,
}

impl Workspace {
    /// Opens every repo directly under `root`, sorted by name. Those `is_archived` picks
    /// (given each repo's absolute folder) are opened without running `git status`.
    pub fn scan(
        root: impl Into<PathBuf>,
        is_archived: impl Fn(&Path) -> bool,
    ) -> Result<Workspace, GitError> {
        let root = absolute(root.into())?;
        let folder = read_dir(&root)?;
        let mut repos = Vec::new();
        let mut archived = Vec::new();

        for entry in folder {
            match entry {
                Ok(entry) => {
                    // Absolute, since `root` is.
                    let path = entry.path();
                    if path.join(".git").is_dir() {
                        let (repo, list) = if is_archived(&path) {
                            (Repo::open_without_status(&path), &mut archived)
                        } else {
                            (Repo::open(&path), &mut repos)
                        };
                        match repo {
                            Ok(repo) => list.push(repo),
                            Err(err) => eprintln!("Error opening {}: {}", path.display(), err),
                        }
                    }
                }
                Err(err) => eprintln!("Error reading directory entry: {}", err),
            }
        }

        repos.sort_by(|a, b| a.name().cmp(b.name()));
        archived.sort_by(|a, b| a.name().cmp(b.name()));

        Ok(Workspace { repos, archived })
    }

    pub fn find_worktree(&self, path: &Path) -> Option<&Worktree> {
        self.repos
            .iter()
            .flat_map(|repo| repo.worktrees())
            .find(|worktree| worktree.path() == path)
    }

    pub fn find_worktree_mut(&mut self, path: &Path) -> Option<&mut Worktree> {
        self.repos
            .iter_mut()
            .flat_map(|repo| repo.worktrees_mut())
            .find(|worktree| worktree.path() == path)
    }

    pub fn find_repo(&self, worktree_path: &Path) -> Option<&Repo> {
        self.repos.iter().find(|repo| {
            repo.worktrees()
                .iter()
                .any(|worktree| worktree.path() == worktree_path)
        })
    }

    /// Moves a repo to the archived list, without any git call: no rescan, so archiving
    /// is instant. Its worktrees' statuses stay as they were; nothing reads them.
    pub fn archive(&mut self, path: &Path) {
        if let Some(index) = self.repos.iter().position(|repo| repo.path() == path) {
            let repo = self.repos.remove(index);
            insert_by_name(&mut self.archived, repo);
        }
    }

    /// Moves an archived repo back, reopened so its worktree list and statuses are
    /// current: nothing kept them up to date while it was archived. On error it stays
    /// archived.
    pub fn unarchive(&mut self, path: &Path) -> Result<(), GitError> {
        if let Some(index) = self.archived.iter().position(|repo| repo.path() == path) {
            let repo = Repo::open(path)?;
            self.archived.remove(index);
            insert_by_name(&mut self.repos, repo);
        }
        Ok(())
    }

    /// The repos in use, which everything else (watching, fetching, selecting) works on.
    pub fn repos(&self) -> &[Repo] {
        &self.repos
    }

    pub fn archived(&self) -> &[Repo] {
        &self.archived
    }
}

/// Inserts `repo` where it keeps `list` sorted by name, as `scan` leaves it.
fn insert_by_name(list: &mut Vec<Repo>, repo: Repo) {
    let index = list.partition_point(|other| other.name() < repo.name());
    list.insert(index, repo);
}
