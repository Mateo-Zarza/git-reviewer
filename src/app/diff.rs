//! The right panel: the selected file's diff.

use std::borrow::Cow;

use iced::advanced::graphics::text::Paragraph;
use iced::advanced::text::{self as core_text, Paragraph as _};
use iced::widget::scrollable::Direction;
use iced::widget::text::{IntoFragment, LineHeight, Shaping, Wrapping};
use iced::widget::{
    Text, column, container, responsive, row, rule, scrollable, space, stack, text,
};
use iced::{
    Alignment::Center,
    Color, Element, Fill, Padding, Pixels, Size,
    alignment::{Horizontal::Right, Vertical},
};
use iced_palace::widget::ellipsized_text;

use super::{App, Message, empty_state};
use crate::git::{Diff, DiffLine, DiffSource};
use crate::theme::{self, colors, fonts, layouts, text_sizes};

/// Lets `update` reset the diff's scroll position when another file is opened.
pub(super) const DIFF_SCROLLABLE: &str = "diff";

// Diff geometry, from the mockup: two 44px line-number columns padded 8 and 10 on the
// right, a 16px sign column, 24px after the code.
const OLD_NUMBER_PADDING: f32 = 8.0;
const NEW_NUMBER_PADDING: f32 = 10.0;
const GUTTER_WIDTH: f32 = layouts::LINE_NUMBER_WIDTH * 2.0
    + OLD_NUMBER_PADDING
    + NEW_NUMBER_PADDING
    + layouts::DIFF_SIGN_WIDTH;
const CODE_PADDING_RIGHT: f32 = 24.0;

/// The selected file's diff, laid out the way `diff_body` draws it. `load_diff` builds
/// it once per diff, so `view`, which runs after every message, does no per-line work.
///
/// Each column (old numbers, new numbers, code) is a single text with one line per diff
/// line, not a row of widgets per line: a whole file would be tens of thousands of
/// widgets, rebuilt after every message, and iced has no virtual list. Every line is
/// `DIFF_LINE_HEIGHT` tall, so the columns line up, and the tints behind them are one
/// block per run of same-kind lines.
pub(super) struct DiffView {
    added: usize,
    removed: usize,
    old_numbers: String,
    new_numbers: String,
    code: String,
    runs: Vec<Run>,
    /// How wide the rows must be to fit the longest line, gutter and padding included.
    width: f32,
}

/// Consecutive diff lines of the same kind.
struct Run {
    kind: Kind,
    lines: usize,
    /// The run's +/− signs, one line each (empty for unchanged lines).
    signs: String,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Context,
    Added,
    Removed,
}

impl Kind {
    fn of(line: &DiffLine) -> Kind {
        match line {
            DiffLine::Context { .. } => Kind::Context,
            DiffLine::Added { .. } => Kind::Added,
            DiffLine::Removed { .. } => Kind::Removed,
        }
    }
}

impl DiffView {
    pub(super) fn new(diff: &Diff) -> DiffView {
        let (added, removed) = diff.counts();
        let column = |cell: fn(&DiffLine) -> String| {
            diff.lines.iter().map(cell).collect::<Vec<_>>().join("\n")
        };
        fn number(number: Option<usize>) -> String {
            number.map(|n| n.to_string()).unwrap_or_default()
        }

        let mut runs: Vec<Run> = Vec::new();
        for kind in diff.lines.iter().map(Kind::of) {
            match runs.last_mut() {
                Some(run) if run.kind == kind => run.lines += 1,
                _ => runs.push(Run {
                    kind,
                    lines: 1,
                    signs: String::new(),
                }),
            }
        }
        for run in &mut runs {
            let sign = match run.kind {
                Kind::Context => continue,
                Kind::Added => "+",
                Kind::Removed => "−",
            };
            run.signs = vec![sign; run.lines].join("\n");
        }

        DiffView {
            added,
            removed,
            old_numbers: column(|line| number(line.old_number())),
            new_numbers: column(|line| number(line.new_number())),
            code: column(|line| display_text(line.text())),
            runs,
            width: content_width(diff),
        }
    }
}

impl App {
    /// A header bar (path, which side, +N −N) over the scrollable diff.
    pub(super) fn diff_panel(&self) -> Element<'_, Message> {
        let (Some((path, source)), Some(diff)) = (&self.selected_file, &self.diff) else {
            let clean = matches!(&self.status, Some(Ok(status)) if status.files.is_empty());
            return match (&self.status, clean) {
                (None, _) => space().into(),
                (Some(_), true) => empty_state(
                    "Nothing to review. Everything is committed.",
                    colors::TEXT_MUTED,
                )
                .center(Fill)
                .into(),
                (Some(_), false) => {
                    empty_state("Select a file to see its changes", colors::TEXT_MUTED)
                        .center(Fill)
                        .into()
                }
            };
        };

        let diff = match diff {
            Ok(diff) => diff,
            Err(err) => {
                return empty_state(err.to_string(), colors::DANGER)
                    .center(Fill)
                    .into();
            }
        };

        column![
            diff_header(path, *source, diff),
            rule::horizontal(1).style(theme::divider),
            diff_body(diff),
        ]
        .into()
    }
}

/// How wide the diff's rows must be to fit its longest line, gutter and padding
/// included. Worked out once per diff (in `DiffView::new`), since lines that aren't
/// plain ASCII are measured, which means shaping their text.
fn content_width(diff: &Diff) -> f32 {
    let code = diff
        .lines
        .iter()
        .map(|line| code_width(line.text()))
        .fold(0.0, f32::max);
    GUTTER_WIDTH + code + CODE_PADDING_RIGHT
}

/// How wide a line's text is drawn. Plain ASCII, nearly every line, is all Fira Code,
/// where every glyph has the same advance, so it's just counted. Anything else may be
/// drawn from a fallback font (CJK characters and emoji are wider than Fira Code's
/// glyphs), so it's measured.
fn code_width(line: &str) -> f32 {
    let expanded = expand_tabs(line);
    if expanded.is_ascii() {
        expanded.len() as f32 * layouts::DIFF_CHAR_WIDTH
    } else {
        measure(&display_text(line))
    }
}

/// Measures drawn diff text with iced's own text engine, set up exactly like `mono`,
/// so fallback fonts and ligatures count the same as on screen.
fn measure(shown: &str) -> f32 {
    Paragraph::with_text(core_text::Text {
        content: shown,
        bounds: Size::INFINITE,
        size: Pixels(text_sizes::SMALL),
        line_height: LineHeight::Absolute(Pixels(layouts::DIFF_LINE_HEIGHT)),
        font: fonts::MONO,
        align_x: core_text::Alignment::Left,
        align_y: Vertical::Top,
        shaping: Shaping::Advanced,
        wrapping: Wrapping::None,
    })
    .min_width()
}

fn diff_header<'a>(path: &'a str, source: DiffSource, diff: &DiffView) -> Element<'a, Message> {
    let side = match source {
        DiffSource::Staged => "Staged",
        DiffSource::Unstaged => "Unstaged",
        DiffSource::Untracked => "Untracked",
    };

    container(
        row![
            ellipsized_text(path)
                .size(text_sizes::SMALL)
                .wrapping(Wrapping::None),
            container(text(side).size(text_sizes::LABEL))
                .padding([1.0, 6.0])
                .style(theme::filled_tag),
            space().width(Fill),
            text(format!("+{}", diff.added))
                .size(text_sizes::SMALL)
                .color(colors::ADDED),
            text(format!("−{}", diff.removed))
                .size(text_sizes::SMALL)
                .color(colors::DANGER),
        ]
        .spacing(10)
        .align_y(Center),
    )
    .height(layouts::DIFF_HEADER_HEIGHT)
    .padding([0.0, layouts::PANEL_PADDING_X])
    .align_y(Center)
    .into()
}

/// The whole file, scrollable both ways: the gutter (line numbers and signs) and the
/// code, each a few texts over a column of tint blocks (see `DiffView`).
///
/// Lines don't wrap, so the code is as wide as its longest line (`DiffView::width`), or
/// the panel if that's wider, so the tints run edge to edge. (Nothing can just be `Fill`:
/// inside a both-ways scrollable that lays out empty.)
fn diff_body(diff: &DiffView) -> Element<'_, Message> {
    responsive(move |size| {
        let tint_width = diff.width.max(size.width) - GUTTER_WIDTH;

        // In a `stack`, the first layer sets the size, and the rest are drawn over it.
        let gutter = stack![
            column(diff.runs.iter().map(gutter_run)),
            row![
                line_numbers(&diff.old_numbers, OLD_NUMBER_PADDING),
                line_numbers(&diff.new_numbers, NEW_NUMBER_PADDING),
            ],
        ];
        let code = stack![
            column(diff.runs.iter().map(|run| code_tint(run, tint_width))),
            // A fixed width, so the text's bounds don't change when the panel is
            // resized: that would make iced lay out every line again.
            mono(diff.code.as_str()).width(diff.width - GUTTER_WIDTH),
        ];

        scrollable(row![gutter, code].padding(Padding::ZERO.bottom(8.0)))
            .id(DIFF_SCROLLABLE)
            .direction(Direction::Both {
                vertical: theme::thin_scrollbar(),
                horizontal: theme::thin_scrollbar(),
            })
            .style(theme::scrollbar)
            .width(Fill)
            .height(Fill)
            .into()
    })
    .into()
}

/// The height of `lines` diff lines.
fn lines_height(lines: usize) -> f32 {
    lines as f32 * layouts::DIFF_LINE_HEIGHT
}

/// One run's stretch of the gutter: its background, with the run's +/− signs in the
/// sign column at its right edge.
fn gutter_run(run: &Run) -> Element<'_, Message> {
    let (background, sign_color) = match run.kind {
        Kind::Context => (colors::BACKGROUND, colors::LINE_NUMBER),
        Kind::Added => (colors::ADDED_GUTTER, colors::ADDED),
        Kind::Removed => (colors::REMOVED_GUTTER, colors::DELETED),
    };

    container(mono(run.signs.as_str()).color(sign_color))
        .padding(Padding::ZERO.left(GUTTER_WIDTH - layouts::DIFF_SIGN_WIDTH))
        .width(GUTTER_WIDTH)
        .height(lines_height(run.lines))
        .style(move |_| container::background(background))
        .into()
}

/// One run's tint behind the code.
fn code_tint(run: &Run, width: f32) -> Element<'_, Message> {
    let tint = match run.kind {
        Kind::Context => Color::TRANSPARENT,
        Kind::Added => colors::ADDED_TINT,
        Kind::Removed => colors::REMOVED_TINT,
    };

    container(space())
        .width(width)
        .height(lines_height(run.lines))
        .style(move |_| container::background(tint))
        .into()
}

/// A column of line numbers, one per line, right-aligned.
fn line_numbers(numbers: &str, padding_right: f32) -> Element<'_, Message> {
    container(
        mono(numbers)
            .color(colors::LINE_NUMBER)
            .width(layouts::LINE_NUMBER_WIDTH)
            .align_x(Right),
    )
    .padding(Padding::ZERO.right(padding_right))
    .into()
}

/// Diff text: Fira Code on a fixed line height, never wrapped.
///
/// `Shaping::Advanced` is what turns on Fira Code's ligatures (`=>`, `->`, `!=` drawn as
/// one symbol, as in VS Code). iced's default, `Auto`, shapes ASCII-only text with
/// `Basic`, which skips them, and nearly all code is ASCII. A ligature keeps the width
/// of the characters it replaces, so the line-width math is unaffected.
fn mono<'a>(content: impl IntoFragment<'a>) -> Text<'a> {
    text(content)
        .font(fonts::MONO)
        .size(text_sizes::SMALL)
        .line_height(Pixels(layouts::DIFF_LINE_HEIGHT))
        .wrapping(Wrapping::None)
        .shaping(Shaping::Advanced)
}

/// Tabs as 4 spaces (VS Code's default), so they render predictably and the width math
/// above (one glyph = one column) holds.
fn expand_tabs(line: &str) -> Cow<'_, str> {
    if line.contains('\t') {
        Cow::Owned(line.replace('\t', "    "))
    } else {
        Cow::Borrowed(line)
    }
}

/// A diff line as it's drawn: tabs expanded, carriage returns dropped, and an invisible
/// word joiner (U+2060) between every two adjacent ASCII punctuation characters.
///
/// A `\r` left inside a line (a stray one, or a file with mixed line endings) would
/// start a new line in the text engine, and every line below it would sit one line off
/// its line numbers, since each column is one text (see `DiffView`).
///
/// The joiners make Fira Code's ligatures work for every operator. The text engine
/// (cosmic-text) shapes each run between Unicode line-break opportunities separately,
/// and there's one after `-` and `!`, so `->` and `!=` reached the font as two pieces
/// and never combined (`=>` did). A word joiner forbids the break, so the operator is
/// shaped as one. It's zero-width, so the width math is unchanged; checked by rendering
/// a line of operators with and without joiners and comparing their ends.
fn display_text(line: &str) -> String {
    let line = expand_tabs(line);
    let mut shown = String::with_capacity(line.len() + line.len() / 4);
    let mut previous_is_punctuation = false;

    for character in line.chars().filter(|&character| character != '\r') {
        let is_punctuation = character.is_ascii_punctuation();
        if previous_is_punctuation && is_punctuation {
            shown.push('\u{2060}');
        }
        shown.push(character);
        previous_is_punctuation = is_punctuation;
    }
    shown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_joiners_go_between_adjacent_punctuation_only() {
        assert_eq!(display_text("a -> b"), "a -\u{2060}> b");
        assert_eq!(display_text("x != y"), "x !\u{2060}= y");
        assert_eq!(display_text("<!--"), "<\u{2060}!\u{2060}-\u{2060}-");
        assert_eq!(display_text("a - b"), "a - b");
        assert_eq!(display_text("let x = 1;"), "let x = 1;");
    }

    #[test]
    fn test_tabs_are_expanded_first() {
        assert_eq!(display_text("\tx => y"), "    x =\u{2060}> y");
    }

    #[test]
    fn test_carriage_returns_are_dropped() {
        assert_eq!(display_text("a\rb\r"), "ab");
    }

    #[test]
    fn test_view_has_one_line_per_diff_line_and_runs_cover_them() {
        let diff = Diff {
            lines: vec![
                DiffLine::Context {
                    old: 1,
                    new: 1,
                    text: "same".to_string(),
                },
                DiffLine::Removed {
                    old: 2,
                    text: "old".to_string(),
                },
                DiffLine::Removed {
                    old: 3,
                    text: "older".to_string(),
                },
                DiffLine::Added {
                    new: 2,
                    text: "new".to_string(),
                },
                DiffLine::Context {
                    old: 4,
                    new: 3,
                    text: String::new(),
                },
            ],
        };
        let view = DiffView::new(&diff);

        assert_eq!((view.added, view.removed), (1, 2));
        assert_eq!(view.old_numbers, "1\n2\n3\n\n4");
        assert_eq!(view.new_numbers, "1\n\n\n2\n3");
        assert_eq!(view.code, "same\nold\nolder\nnew\n");
        let runs: Vec<_> = view
            .runs
            .iter()
            .map(|run| (run.lines, run.signs.as_str()))
            .collect();
        assert_eq!(runs, vec![(1, ""), (2, "−\n−"), (1, "+"), (1, "")]);
    }

    /// Loads Fira Code into iced's font system, as the app does at startup.
    fn load_fira_code() {
        iced::advanced::graphics::text::font_system()
            .write()
            .expect("font system")
            .load_font(Cow::Borrowed(include_bytes!(
                "../../assets/fonts/FiraCode-Regular.ttf"
            )));
    }

    #[test]
    fn test_counted_ascii_width_matches_the_measured_one() {
        load_fira_code();
        let line = "\tlet x = a -> b != c; // <= => 0x1F";
        let measured = measure(&display_text(line));
        assert!((code_width(line) - measured).abs() < 0.01, "{measured}");
    }
}
