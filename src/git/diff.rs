/// One line of a whole-file diff. Each line keeps its number on the side(s) it exists
/// on, so the old file (context + removed lines, in order) and the new file (context +
/// added lines) can both be read back out of a `Diff`.
#[derive(Debug, Clone, PartialEq)]
pub enum DiffLine {
    Context {
        old: usize,
        new: usize,
        text: String,
    }, // unchanged line
    Added {
        new: usize,
        text: String,
    },
    Removed {
        old: usize,
        text: String,
    },
}

impl DiffLine {
    /// The line's content, without the +/-/space prefix.
    pub fn text(&self) -> &str {
        match self {
            DiffLine::Context { text, .. }
            | DiffLine::Added { text, .. }
            | DiffLine::Removed { text, .. } => text,
        }
    }

    /// The line's number in the old file, if it's in the old file (not an added line).
    pub fn old_number(&self) -> Option<usize> {
        match self {
            DiffLine::Context { old, .. } | DiffLine::Removed { old, .. } => Some(*old),
            DiffLine::Added { .. } => None,
        }
    }

    /// The line's number in the new file, if it's in the new file (not a removed line).
    pub fn new_number(&self) -> Option<usize> {
        match self {
            DiffLine::Context { new, .. } | DiffLine::Added { new, .. } => Some(*new),
            DiffLine::Removed { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DiffSource {
    Staged,
    Unstaged,
    Untracked,
}

/// A file's diff as the whole file, changes inline (like VS Code), not hunks: git is
/// asked for enough context that one hunk covers everything (see `Worktree::diff`).
#[derive(Debug, Clone, PartialEq)]
pub struct Diff {
    pub lines: Vec<DiffLine>,
}
impl Diff {
    /// (added, removed) line counts, for the diff header's +N −N.
    pub fn counts(&self) -> (usize, usize) {
        self.lines
            .iter()
            .fold((0, 0), |(added, removed), line| match line {
                DiffLine::Added { .. } => (added + 1, removed),
                DiffLine::Removed { .. } => (added, removed + 1),
                _ => (added, removed),
            })
    }

    pub(super) fn from_new_file(content: &str) -> Diff {
        let lines = content
            .lines()
            .enumerate()
            .map(|(i, line)| DiffLine::Added {
                new: i + 1,
                text: line.to_string(),
            })
            .collect();

        Diff { lines }
    }

    pub(super) fn parse(output: &str) -> Diff {
        let lines = output.lines();
        // Skip everything until the first hunk header
        let lines = lines.skip_while(|line| !line.starts_with("@@"));
        let mut diff = Vec::new();
        let (mut old, mut new) = (0, 0);

        for line in lines {
            match line.chars().next() {
                // Not shown: a hunk header only says where its line numbers start.
                Some('@') => {
                    if let Some((old_start, new_start)) = Diff::parse_hunk_header(line) {
                        old = old_start;
                        new = new_start;
                    }
                }
                Some('+') => {
                    diff.push(DiffLine::Added {
                        new,
                        text: line[1..].to_string(),
                    });
                    new += 1;
                }
                Some('-') => {
                    diff.push(DiffLine::Removed {
                        old,
                        text: line[1..].to_string(),
                    });
                    old += 1;
                }
                Some(' ') => {
                    diff.push(DiffLine::Context {
                        old,
                        new,
                        text: line[1..].to_string(),
                    });
                    old += 1;
                    new += 1;
                }
                _ => {} // ignore other lines
            };
        }

        Diff { lines: diff }
    }

    fn parse_hunk_header(line: &str) -> Option<(usize, usize)> {
        if !line.starts_with("@@") {
            return None;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();

        let old_start = parts
            .get(1)?
            .strip_prefix('-')?
            .split(',')
            .next()?
            .parse::<usize>()
            .ok()?;
        let new_start = parts
            .get(2)?
            .strip_prefix('+')?
            .split(',')
            .next()?
            .parse::<usize>()
            .ok()?;

        Some((old_start, new_start))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_diff() {
        let output = "diff --git a/src/app.rs b/src/app.rs
index 7ae3a89..2b51d74 100644
--- a/src/app.rs
+++ b/src/app.rs
@@ -38,7 +38,7 @@ impl App {
     pub fn view(&self) -> Element<'_, Message> {
-            Ok(workspace) => column![
+            Ok(workspace) => row![
";
        let diff = Diff::parse(output);
        assert_eq!(
            diff.lines,
            vec![
                DiffLine::Context {
                    old: 38,
                    new: 38,
                    text: "    pub fn view(&self) -> Element<'_, Message> {".to_string()
                },
                DiffLine::Removed {
                    old: 39,
                    text: "            Ok(workspace) => column![".to_string()
                },
                DiffLine::Added {
                    new: 39,
                    text: "            Ok(workspace) => row![".to_string()
                },
            ]
        );
        assert_eq!(diff.counts(), (1, 1));
        assert_eq!(
            diff.lines[1].text(),
            "            Ok(workspace) => column!["
        );
    }

    #[test]
    fn test_parse_skips_no_newline_marker() {
        let output = "@@ -1,2 +1,2 @@
 kept
-old end
\\ No newline at end of file
+new end
\\ No newline at end of file
";
        let diff = Diff::parse(output);
        assert_eq!(
            diff.lines,
            vec![
                DiffLine::Context {
                    old: 1,
                    new: 1,
                    text: "kept".to_string()
                },
                DiffLine::Removed {
                    old: 2,
                    text: "old end".to_string()
                },
                DiffLine::Added {
                    new: 2,
                    text: "new end".to_string()
                },
            ]
        );
    }

    #[test]
    fn test_parse_hunk_header() {
        let header = "@@ -38,7 +38,7 @@ impl App {";
        let result = Diff::parse_hunk_header(header);
        assert_eq!(result, Some((38, 38)));

        let invalid_header = "@@ -abc,7 +def,7 @@ impl App {";
        let result = Diff::parse_hunk_header(invalid_header);
        assert_eq!(result, None);

        let not_a_header = "This is not a hunk header";
        let result = Diff::parse_hunk_header(not_a_header);
        assert_eq!(result, None);

        let empty_header = "@@ -5 +6 @@";
        let result = Diff::parse_hunk_header(empty_header);
        assert_eq!(result, Some((5, 6)));
    }

    #[test]
    fn test_from_new_file() {
        let content = "Line 1\nLine 2\nLine 3";
        let diff = Diff::from_new_file(content);
        assert_eq!(
            diff.lines,
            vec![
                DiffLine::Added {
                    new: 1,
                    text: "Line 1".to_string()
                },
                DiffLine::Added {
                    new: 2,
                    text: "Line 2".to_string()
                },
                DiffLine::Added {
                    new: 3,
                    text: "Line 3".to_string()
                },
            ]
        );
        assert_eq!(diff.counts(), (3, 0));
    }
}
