use std::{
    fs::read_dir,
    path::{Path, PathBuf, absolute},
};

use crate::git::{GitError, Repo, Worktree};

pub struct Workspace {
    repos: Vec<Repo>,
    /// Repos set aside by the user: their worktrees are listed but carry no status, and
    /// the `find_*` lookups never return them.
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

    /// The repos in use, which everything else (watching, fetching, selecting) works on.
    pub fn repos(&self) -> &[Repo] {
        &self.repos
    }

    pub fn archived(&self) -> &[Repo] {
        &self.archived
    }
}
