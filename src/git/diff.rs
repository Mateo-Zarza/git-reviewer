#[derive(Debug, PartialEq)]
pub enum DiffLine {
    Hunk(String),    // "@@ -38,7 +38,7 @@ impl App {"
    Context(String), // unchanged line
    Added(String),
    Removed(String),
}

#[derive(Debug, PartialEq)]
pub struct Diff {
    pub lines: Vec<DiffLine>,
}
impl Diff {
    pub(super) fn parse(output: &str) -> Diff {
        let lines = output.lines();
        // Skip everything until the first hunk header
        let lines = lines.skip_while(|line| !line.starts_with("@@"));
        let mut diff = Vec::new();

        for line in lines {
            match line.chars().next() {
                Some('+') => diff.push(DiffLine::Added(line[1..].to_string())),
                Some('-') => diff.push(DiffLine::Removed(line[1..].to_string())),
                Some('@') => diff.push(DiffLine::Hunk(line.to_string())),
                Some(' ') => diff.push(DiffLine::Context(line[1..].to_string())),
                _ => {} // ignore other lines
            };
        }

        Diff { lines: diff }
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
                DiffLine::Hunk("@@ -38,7 +38,7 @@ impl App {".to_string()),
                DiffLine::Context("    pub fn view(&self) -> Element<'_, Message> {".to_string()),
                DiffLine::Removed("            Ok(workspace) => column![".to_string()),
                DiffLine::Added("            Ok(workspace) => row![".to_string()),
            ]
        );
    }
}
