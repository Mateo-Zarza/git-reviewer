#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    // Staged changes
    Added,
    Renamed,
    Copied,
    // Unstaged changes
    Untracked,
    // Both staged and unstaged changes
    Modified,
    Deleted,
    TypeChanged,
}
impl ChangeKind {
    fn from_code(code: char) -> Option<ChangeKind> {
        match code {
            'A' => Some(ChangeKind::Added),
            'R' => Some(ChangeKind::Renamed),
            'C' => Some(ChangeKind::Copied),
            'M' => Some(ChangeKind::Modified),
            'D' => Some(ChangeKind::Deleted),
            'T' => Some(ChangeKind::TypeChanged),
            _ => None,
        }
    }

    pub fn to_code(self) -> char {
        match self {
            ChangeKind::Added => 'A',
            ChangeKind::Renamed => 'R',
            ChangeKind::Copied => 'C',
            ChangeKind::Modified => 'M',
            ChangeKind::Deleted => 'D',
            ChangeKind::TypeChanged => 'T',
            ChangeKind::Untracked => 'U',
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upstream {
    pub name: String,
    pub counts: Option<(usize, usize)>, // (ahead, behind)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub branch: Option<String>,
    pub files: Vec<FileEntry>,
    pub upstream: Option<Upstream>,
}
impl Status {
    /// Number of entries across the staged and unstaged groups, like VS Code's badge:
    /// a file with both staged and unstaged changes counts twice.
    pub fn change_count(&self) -> usize {
        self.files
            .iter()
            .map(|file| file.staged.is_some() as usize + file.unstaged.is_some() as usize)
            .sum()
    }

    /// The branch tracks an upstream that no longer exists on the remote
    /// (git prints `branch.upstream` but no `branch.ab`).
    pub fn is_upstream_gone(&self) -> bool {
        self.upstream.as_ref().is_some_and(|u| u.counts.is_none())
    }

    pub(super) fn parse(output: &str) -> Status {
        let mut lines = output.split('\0');

        let mut branch = None;
        let mut files = Vec::new();
        let mut upstream = None;

        while let Some(line) = lines.next() {
            match line.split_once(' ') {
                Some(("#", header)) => match header.split_once(' ') {
                    Some(("branch.head", "(detached)")) => branch = None,
                    Some(("branch.head", name)) => branch = Some(name.to_string()),
                    Some(("branch.upstream", name)) => {
                        upstream = Some(Upstream {
                            name: name.to_string(),
                            counts: None,
                        })
                    }
                    Some(("branch.ab", counts)) => {
                        if let Some(u) = upstream.as_mut() {
                            u.counts = Status::parse_upstream_ab(counts);
                        }
                    }
                    _ => {} // branch.oid, or headers we don't use
                },
                Some(("1", rest)) => {
                    if let Some(file_entry) = Status::parse_file_entry(rest, false) {
                        files.push(file_entry);
                    }
                }
                Some(("2", rest)) => {
                    if let Some(file_entry) = Status::parse_file_entry(rest, true) {
                        files.push(FileEntry {
                            old_path: lines.next().map(|s| s.to_string()),
                            ..file_entry
                        });
                    }
                }
                Some(("?", path)) => files.push(FileEntry {
                    path: path.to_string(),
                    old_path: None,
                    staged: None,
                    unstaged: Some(ChangeKind::Untracked),
                }),
                _ => {} // conflicts, the empty last piece, anything unknown
            }
        }

        Status {
            branch,
            files,
            upstream,
        }
    }

    fn parse_upstream_ab(value: &str) -> Option<(usize, usize)> {
        let (ahead, behind) = value.split_once(' ')?;
        let ahead = ahead.strip_prefix('+')?.parse::<usize>().ok()?;
        let behind = behind.strip_prefix('-')?.parse::<usize>().ok()?;
        Some((ahead, behind))
    }

    fn parse_file_entry(value: &str, is_renamed: bool) -> Option<FileEntry> {
        let mut parts = value.splitn(if is_renamed { 9 } else { 8 }, ' ');
        let xy = parts.next()?;

        let path = parts.last()?.to_string();
        let old_path = None;
        let staged = ChangeKind::from_code(xy.chars().next()?);
        let unstaged = ChangeKind::from_code(xy.chars().nth(1)?);

        Some(FileEntry {
            path,
            old_path,
            staged,
            unstaged,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub path: String,
    pub old_path: Option<String>,
    pub staged: Option<ChangeKind>,
    pub unstaged: Option<ChangeKind>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_change_kind_from_code() {
        assert_eq!(ChangeKind::from_code('A'), Some(ChangeKind::Added));
        assert_eq!(ChangeKind::from_code('R'), Some(ChangeKind::Renamed));
        assert_eq!(ChangeKind::from_code('C'), Some(ChangeKind::Copied));
        assert_eq!(ChangeKind::from_code('M'), Some(ChangeKind::Modified));
        assert_eq!(ChangeKind::from_code('D'), Some(ChangeKind::Deleted));
        assert_eq!(ChangeKind::from_code('T'), Some(ChangeKind::TypeChanged));
        assert_eq!(ChangeKind::from_code('U'), None);
        assert_eq!(ChangeKind::from_code('.'), None);
    }

    #[test]
    fn test_parse() {
        let output = "# branch.oid f446061e...\0\
                            # branch.head main\0\
                            # branch.upstream origin/main\0\
                            # branch.ab +2 -1\0\
                            1 MM N... 100644 100644 100644 7898192... 93829c7... app.rs\0\
                            1 .D N... 100644 100644 000000 4bcfe98... 4bcfe98... gone.rs\0\
                            1 .M N... 100644 100644 100644 f2ad6c7... f2ad6c7... lib.rs\0\
                            2 R. N... 100644 100644 100644 6178079... 6178079... R100 new_name.rs\0\
                            old_name.rs\0\
                            ? notes todo.txt\0";

        let status = Status::parse(output);

        assert_eq!(status.branch, Some("main".to_string()));
        assert_eq!(
            status.upstream,
            Some(Upstream {
                name: "origin/main".to_string(),
                counts: Some((2, 1)),
            })
        );
        assert_eq!(status.files.len(), 5);

        assert_eq!(status.files[0].path, "app.rs");
        assert_eq!(status.files[0].staged, Some(ChangeKind::Modified));
        assert_eq!(status.files[0].unstaged, Some(ChangeKind::Modified));
        assert_eq!(status.files[0].old_path, None);

        assert_eq!(status.files[1].path, "gone.rs");
        assert_eq!(status.files[1].staged, None);
        assert_eq!(status.files[1].unstaged, Some(ChangeKind::Deleted));
        assert_eq!(status.files[1].old_path, None);

        assert_eq!(status.files[2].path, "lib.rs");
        assert_eq!(status.files[2].staged, None);
        assert_eq!(status.files[2].unstaged, Some(ChangeKind::Modified));
        assert_eq!(status.files[2].old_path, None);

        assert_eq!(status.files[3].path, "new_name.rs");
        assert_eq!(status.files[3].staged, Some(ChangeKind::Renamed));
        assert_eq!(status.files[3].unstaged, None);
        assert_eq!(status.files[3].old_path, Some("old_name.rs".to_string()));

        assert_eq!(status.files[4].path, "notes todo.txt");
        assert_eq!(status.files[4].staged, None);
        assert_eq!(status.files[4].unstaged, Some(ChangeKind::Untracked));
        assert_eq!(status.files[4].old_path, None);

        // app.rs is MM, so it counts once per group: 2 + 1 + 1 + 1 + 1
        assert_eq!(status.change_count(), 6);
        assert!(!status.is_upstream_gone());
    }

    #[test]
    fn test_parse_no_upstream() {
        let output = "# branch.oid f446061e...\0\
                            # branch.head feat/new\0";

        let status = Status::parse(output);

        assert_eq!(status.branch, Some("feat/new".to_string()));
        assert_eq!(status.upstream, None);
        assert!(!status.is_upstream_gone());
        assert_eq!(status.change_count(), 0);
    }

    #[test]
    fn test_parse_upstream_gone() {
        let output = "# branch.oid f446061e...\0\
                            # branch.head fix/login\0\
                            # branch.upstream origin/fix/login\0";

        let status = Status::parse(output);

        assert_eq!(status.branch, Some("fix/login".to_string()));
        assert_eq!(
            status.upstream,
            Some(Upstream {
                name: "origin/fix/login".to_string(),
                counts: None,
            })
        );
        assert!(status.is_upstream_gone());
    }

    #[test]
    fn test_parse_detached_head() {
        let output = "# branch.oid f446061e...\0\
                            # branch.head (detached)\0";

        let status = Status::parse(output);

        assert_eq!(status.branch, None);
    }

    #[test]
    fn test_parse_new_repo_no_commits() {
        let output = "# branch.oid (initial)\0\
                            # branch.head main\0";

        let status = Status::parse(output);

        assert_eq!(status.branch, Some("main".to_string()));
    }
}
