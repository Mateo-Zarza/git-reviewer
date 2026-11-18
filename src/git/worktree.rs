use crate::git::{Diff, DiffSource, GitError, status::Status};

use super::Git;
use std::path::Path;

#[derive(Clone)]
pub struct Worktree {
    git: Git,
    head: String,
    branch: Option<String>,
    is_main: bool,
}
impl Worktree {
    fn new(git: Git, head: String, branch: Option<String>, is_main: bool) -> Self {
        Worktree {
            git,
            head,
            branch,
            is_main,
        }
    }

    pub fn path(&self) -> &Path {
        self.git.dir()
    }

    pub fn branch(&self) -> Option<&str> {
        self.branch.as_deref()
    }

    pub fn head(&self) -> &str {
        &self.head
    }

    pub fn is_main(&self) -> bool {
        self.is_main
    }

    fn parse_block(block: &str, is_main: bool) -> Option<Worktree> {
        let mut path = None;
        let mut head = None;
        let mut branch = None;

        for line in block.lines() {
            if let Some(p) = line.strip_prefix("worktree ") {
                path = Some(p);
            } else if let Some(h) = line.strip_prefix("HEAD ") {
                head = Some(h.to_string());
            } else if let Some(b) = line.strip_prefix("branch ") {
                branch = Some(b.strip_prefix("refs/heads/").unwrap_or(b).to_string());
            }
        }

        Some(Worktree::new(Git::new(path?), head?, branch, is_main))
    }

    pub(super) fn parse_list(output: &str) -> Vec<Worktree> {
        let mut worktrees = Vec::new();
        let blocks = output.split("\n\n");

        for (i, block) in blocks.enumerate() {
            if let Some(worktree) = Worktree::parse_block(block, i == 0) {
                worktrees.push(worktree);
            }
        }
        worktrees
    }

    pub fn status(&self) -> Result<Status, GitError> {
        let output = self
            .git
            .run(&["status", "--porcelain=2", "--branch", "-z"])?;
        Ok(Status::parse(&output))
    }

    pub fn diff(&self, path: &str, source: DiffSource) -> Result<Diff, GitError> {
        Ok(match source {
            DiffSource::Staged => Diff::parse(&self.git.run(&["diff", "--cached", "--", path])?),
            DiffSource::Unstaged => Diff::parse(&self.git.run(&["diff", "--", path])?),
            DiffSource::Untracked => {
                Diff::from_new_file(&std::fs::read_to_string(self.path().join(path))?)
            }
        })
    }

    pub fn stage(&self, path: &str) -> Result<(), GitError> {
        self.git.run(&["add", "--", path])?;
        Ok(())
    }

    pub fn unstage(&self, path: &str) -> Result<(), GitError> {
        self.git.run(&["restore", "--staged", "--", path])?;
        Ok(())
    }

    pub fn commit(&self, message: &str) -> Result<(), GitError> {
        self.git.run(&["commit", "-m", message])?;
        Ok(())
    }

    pub fn pull(&self) -> Result<(), GitError> {
        self.git.run(&["pull"])?;
        Ok(())
    }

    pub fn push(&self) -> Result<(), GitError> {
        self.git.run(&["push"])?;
        Ok(())
    }

    pub fn sync(&self) -> Result<(), GitError> {
        self.pull()?;
        self.push()
    }

    pub fn ahead_behind(&self, base: &str) -> Result<(usize, usize), GitError> {
        let output = self.git.run(&[
            "rev-list",
            "--left-right",
            "--count",
            &format!("HEAD...{base}"),
        ])?;

        Worktree::parse_counts(&output).ok_or_else(|| {
            GitError::CommandFailed(format!(
                "Failed to parse ahead/behind counts from output: {output}",
            ))
        })
    }

    fn parse_counts(output: &str) -> Option<(usize, usize)> {
        let parts: Vec<&str> = output.split_whitespace().collect();

        let ahead = parts.get(0)?.parse::<usize>().ok()?;
        let behind = parts.get(1)?.parse::<usize>().ok()?;
        Some((ahead, behind))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_main_and_one_worktree() {
        let output = "worktree /path/to/main\n\
                            HEAD abcdef1234567890\n\
                            branch refs/heads/main\n\
                            \n\
                            worktree /path/to/feature\n\
                            HEAD 1234567890abcdef\n\
                            branch refs/heads/feature\n\n";
        let worktrees = Worktree::parse_list(output);

        assert_eq!(worktrees.len(), 2);

        let main_worktree = &worktrees[0];
        assert_eq!(main_worktree.path(), Path::new("/path/to/main"));
        assert_eq!(main_worktree.head(), "abcdef1234567890");
        assert_eq!(main_worktree.branch(), Some("main"));
        assert!(main_worktree.is_main());

        let feature_worktree = &worktrees[1];
        assert_eq!(feature_worktree.path(), Path::new("/path/to/feature"));
        assert_eq!(feature_worktree.head(), "1234567890abcdef");
        assert_eq!(feature_worktree.branch(), Some("feature"));
        assert!(!feature_worktree.is_main());
    }

    #[test]
    fn test_detached_worktree() {
        let output = "worktree /path/to/main\n\
                            HEAD abcdef1234567890\n\
                            branch refs/heads/main\n\
                            \n\
                            worktree /path/to/detached\n\
                            HEAD 1234567890abcdef\n\
                            detached\n\n";
        let worktrees = Worktree::parse_list(output);

        assert_eq!(worktrees.len(), 2);

        let detached_worktree = &worktrees[1];
        assert_eq!(detached_worktree.path(), Path::new("/path/to/detached"));
        assert_eq!(detached_worktree.head(), "1234567890abcdef");
        assert_eq!(detached_worktree.branch(), None);
        assert!(!detached_worktree.is_main());
    }

    #[test]
    fn test_empty_output() {
        let output = "";
        let worktrees = Worktree::parse_list(output);
        assert!(worktrees.is_empty());
    }

    #[test]
    fn test_path_with_spaces() {
        let output = "worktree /path/to/main\n\
                            HEAD abcdef1234567890\n\
                            branch refs/heads/main\n\
                            \n\
                            worktree /path/to/feature with spaces\n\
                            HEAD 1234567890abcdef\n\
                            branch refs/heads/feature\n\n";
        let worktrees = Worktree::parse_list(output);

        assert_eq!(worktrees.len(), 2);

        let feature_worktree = &worktrees[1];
        assert_eq!(
            feature_worktree.path(),
            Path::new("/path/to/feature with spaces")
        );
        assert_eq!(feature_worktree.head(), "1234567890abcdef");
        assert_eq!(feature_worktree.branch(), Some("feature"));
        assert!(!feature_worktree.is_main());
    }

    #[test]
    fn test_parse_counts() {
        let output = "1\t1\n";
        let counts = Worktree::parse_counts(output);
        assert_eq!(counts, Some((1, 1)));

        let output = "";
        let counts = Worktree::parse_counts(output);
        assert_eq!(counts, None);
    }
}
