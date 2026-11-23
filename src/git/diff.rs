/// One line of a whole-file diff. Each line keeps its number on the side(s) it exists
/// on, so the old file (context + removed lines, in order) and the new file (context +
/// added lines) can both be read back out of a `Diff`.
#[derive(Debug, PartialEq)]
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
#[derive(Debug, PartialEq)]
pub struct Diff {
    pub lines: Vec<DiffLine>,
    /// git treats the file as binary, so there are no lines to show (VS Code says so
    /// in place of the diff, and so do we).
    pub binary: bool,
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

    /// An untracked file, read straight from disk, as all added lines. It's binary by
    /// git's own rule (a NUL byte in its first 8000 bytes), so binary files are spotted
    /// the same whether tracked or not. Text that isn't valid UTF-8 (say, Latin-1) is
    /// shown with `�` for the bad bytes, as tracked files are (see `Git::run`).
    pub(super) fn from_new_file(content: &[u8]) -> Diff {
        const BINARY_CHECK_BYTES: usize = 8000;
        let head = &content[..content.len().min(BINARY_CHECK_BYTES)];
        if head.contains(&0) {
            return Diff {
                lines: Vec::new(),
                binary: true,
            };
        }

        let lines = String::from_utf8_lossy(content)
            .lines()
            .enumerate()
            .map(|(i, line)| DiffLine::Added {
                new: i + 1,
                text: line.to_string(),
            })
            .collect();

        Diff {
            lines,
            binary: false,
        }
    }

    pub(super) fn parse(output: &str) -> Diff {
        // A binary file gets no hunks, just a `Binary files a/x and b/x differ` header
        // line. A line of a text file's content can't pass for it: in a hunk, every
        // line starts with its `+`, `-` or space.
        let binary = output.lines().any(|line| line.starts_with("Binary files "));

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

        Diff {
            lines: diff,
            binary,
        }
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
        let content = b"Line 1\nLine 2\nLine 3";
        let diff = Diff::from_new_file(content);
        assert!(!diff.binary);
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

    #[test]
    fn test_from_new_file_with_a_nul_byte_is_binary() {
        // A PNG's signature, then a NUL byte, as image files have.
        let content = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR";
        let diff = Diff::from_new_file(content);
        assert!(diff.binary);
        assert!(diff.lines.is_empty());
    }

    #[test]
    fn test_from_new_file_checks_only_the_first_8000_bytes() {
        // Like git: a NUL byte further in doesn't make the file binary.
        let mut content = vec![b'a'; 8000];
        content.extend_from_slice(b"\0\n");
        assert!(!Diff::from_new_file(&content).binary);
    }

    #[test]
    fn test_from_new_file_that_is_not_utf8_is_text() {
        // "café" in Latin-1: é is the single byte 0xE9, invalid on its own in UTF-8.
        let diff = Diff::from_new_file(b"caf\xe9\nok");
        assert!(!diff.binary);
        assert_eq!(
            diff.lines,
            vec![
                DiffLine::Added {
                    new: 1,
                    text: "caf\u{FFFD}".to_string()
                },
                DiffLine::Added {
                    new: 2,
                    text: "ok".to_string()
                },
            ]
        );
    }

    #[test]
    fn test_parse_binary_file() {
        let output = "diff --git a/logo.png b/logo.png
index 3f1a2b4..9c8d7e6 100644
Binary files a/logo.png and b/logo.png differ
";
        let diff = Diff::parse(output);
        assert!(diff.binary);
        assert!(diff.lines.is_empty());
    }

    #[test]
    fn test_parse_text_file_is_not_binary() {
        // A text file whose content says "Binary files ..." is still text.
        let output = "@@ -1 +1 @@
-Binary files a/x and b/x differ
+fixed
";
        assert!(!Diff::parse(output).binary);
    }
}
