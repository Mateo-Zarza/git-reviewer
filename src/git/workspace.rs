use std::{
    fs::read_dir,
    path::{Path, PathBuf, absolute},
};

use crate::git::{GitError, Repo};

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

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn repos(&self) -> &[Repo] {
        &self.repos
    }
}
