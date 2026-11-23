//! The right panel: the selected file's diff.

use std::borrow::Cow;

use iced::advanced::graphics::text::Paragraph;
use iced::advanced::text::{self as core_text, Paragraph as _};
use iced::widget::scrollable::Direction;
use iced::widget::text::{IntoFragment, LineHeight, Shaping, Wrapping};
use iced::widget::{Text, column, container, responsive, row, rule, scrollable, space, text};
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
// right, a 16px sign column, 24px after the code; hunk headers are padded 16 each side.
const OLD_NUMBER_PADDING: f32 = 8.0;
const NEW_NUMBER_PADDING: f32 = 10.0;
const GUTTER_WIDTH: f32 = layouts::LINE_NUMBER_WIDTH * 2.0
    + OLD_NUMBER_PADDING
    + NEW_NUMBER_PADDING
    + layouts::DIFF_SIGN_WIDTH;
const CODE_PADDING_RIGHT: f32 = 24.0;
const HUNK_PADDING_X: f32 = 16.0;

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
            diff_body(diff, self.diff_width),
        ]
        .into()
    }
}

/// How wide the diff's rows must be to fit its longest line, gutter and padding
/// included. `load_diff` works it out once per diff rather than `view` on every redraw,
/// since lines that aren't plain ASCII are measured, which means shaping their text.
pub(super) fn content_width(diff: &Diff) -> f32 {
    diff.lines
        .iter()
        .map(|line| {
            let code = code_width(line.text());
            match line {
                DiffLine::Hunk(_) => code + HUNK_PADDING_X * 2.0,
                _ => GUTTER_WIDTH + code + CODE_PADDING_RIGHT,
            }
        })
        .fold(0.0, f32::max)
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

fn diff_header<'a>(path: &'a str, source: DiffSource, diff: &Diff) -> Element<'a, Message> {
    let (added, removed) = diff.counts();
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
            text(format!("+{added}"))
                .size(text_sizes::SMALL)
                .color(colors::ADDED),
            text(format!("−{removed}"))
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

/// The diff lines, scrollable both ways. Lines don't wrap, so every row is given the same
/// width: the longest line's (`content_width`), or the panel's if that's wider, so the
/// tints run edge to edge. (Rows can't just be `Fill`: inside a both-ways scrollable
/// that lays out empty.)
fn diff_body(diff: &Diff, content_width: f32) -> Element<'_, Message> {
    responsive(move |size| {
        let width = content_width.max(size.width);

        // The first hunk header sits right under the header bar: no padding on top.
        let lines = diff
            .lines
            .iter()
            .enumerate()
            .map(|(index, line)| diff_line_view(line, width, index == 0));

        scrollable(column(lines).padding(Padding::ZERO.bottom(8.0)))
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

/// One diff line. `first` is true for the diff's first line (always a hunk header).
fn diff_line_view(line: &DiffLine, width: f32, first: bool) -> Element<'_, Message> {
    let (old, new, sign, gutter_bg, line_bg, sign_color) = match line {
        DiffLine::Hunk(header) => {
            // A gap only above later hunks, to set them apart from the lines before;
            // the lines below follow with no gap, like VS Code.
            let gap_above = if first { 0.0 } else { 6.0 };
            return container(
                container(mono(display_text(header)).color(colors::HUNK_TEXT))
                    .padding([2.0, HUNK_PADDING_X])
                    .width(width)
                    .style(|_| container::background(colors::HUNK_BG)),
            )
            .padding(Padding::ZERO.top(gap_above))
            .into();
        }
        DiffLine::Context { old, new, .. } => (
            Some(*old),
            Some(*new),
            " ",
            colors::BACKGROUND,
            Color::TRANSPARENT,
            colors::LINE_NUMBER,
        ),
        DiffLine::Added { new, .. } => (
            None,
            Some(*new),
            "+",
            colors::ADDED_GUTTER,
            colors::ADDED_TINT,
            colors::ADDED,
        ),
        DiffLine::Removed { old, .. } => (
            Some(*old),
            None,
            "−",
            colors::REMOVED_GUTTER,
            colors::REMOVED_TINT,
            colors::DELETED,
        ),
    };

    let number = |number: Option<usize>, padding_right: f32| {
        container(
            mono(number.map(|n| n.to_string()).unwrap_or_default())
                .color(colors::LINE_NUMBER)
                .width(layouts::LINE_NUMBER_WIDTH)
                .align_x(Right),
        )
        .padding(Padding::ZERO.right(padding_right))
    };

    container(row![
        container(row![
            number(old, OLD_NUMBER_PADDING),
            number(new, NEW_NUMBER_PADDING),
            mono(sign).color(sign_color).width(layouts::DIFF_SIGN_WIDTH),
        ])
        .style(move |_| container::background(gutter_bg)),
        mono(display_text(line.text())),
    ])
    .width(width)
    .style(move |_| container::background(line_bg))
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

/// A diff line as it's drawn: tabs expanded, and an invisible word joiner (U+2060)
/// between every two adjacent ASCII punctuation characters.
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

    for character in line.chars() {
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
