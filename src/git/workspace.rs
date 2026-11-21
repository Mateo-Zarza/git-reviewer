use std::{
    fs::read_dir,
    path::{Path, PathBuf, absolute},
};

use crate::git::{GitError, Repo, Worktree};

pub struct Workspace {
    root: PathBuf,
    repos: Vec<Repo>,
}

impl Workspace {
    pub fn scan(root: impl Into<PathBuf>) -> Result<Workspace, GitError> {
        let root = absolute(root.into())?;
        let folder = read_dir(&root)?;
        let mut repos = Vec::new();

        for entry in folder {
            match entry {
                Ok(entry) => {
                    let path = entry.path();
                    if path.join(".git").is_dir() {
                        let repo = Repo::open(&path);
                        match repo {
                            Ok(repo) => repos.push(repo),
                            Err(err) => eprintln!("Error opening {}: {}", path.display(), err),
                        }
                    }
                }
                Err(err) => eprintln!("Error reading directory entry: {}", err),
            }
        }

        repos.sort_by(|a, b| a.name().cmp(b.name()));

        Ok(Workspace { root, repos })
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

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn repos(&self) -> &[Repo] {
        &self.repos
    }
}
