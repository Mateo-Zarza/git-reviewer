//! The right panel: the selected file's diff.

use std::borrow::Cow;
use std::ops::Range;
use std::sync::Arc;
use std::thread;

use iced::advanced::graphics::text::Paragraph;
use iced::advanced::text::{self as core_text, Paragraph as _};
use iced::futures::channel::oneshot;
use iced::widget::scrollable::Direction;
use iced::widget::text::{IntoFragment, LineHeight, Shaping, Span, Wrapping};
use iced::widget::{
    Text, column, container, responsive, rich_text, row, rule, scrollable, space, span, stack, text,
};
use iced::{
    Alignment::Center,
    Color, Element, Fill, Padding, Pixels, Size, Task,
    alignment::{Horizontal::Right, Vertical},
};
use iced_palace::widget::ellipsized_text;

use super::sticky::sticky;
use super::{App, Message, empty_state};
use crate::git::{Diff, DiffLine, DiffSource};
use crate::highlight::{self, LineColors};
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

/// How many diff lines go in one `Chunk`.
const CHUNK_LINES: usize = 64;

/// The selected file's diff, laid out the way `diff_body` draws it. `load_diff` builds
/// it once per diff, so `view`, which runs after every message, does no per-line work.
///
/// The text is cut into chunks of `CHUNK_LINES` lines, and only the chunks around what's
/// visible are put on screen (see `ChunkWindow`). Shaping text is what costs (around
/// 60ms per 1000 lines in a release build, measured), and it happens for every text
/// widget that's laid out, visible or not: shaping a whole 5000-line file took over
/// 0.3s, and much longer in a debug build. Behind the text, the tints are one block per
/// run of same-kind lines for the whole file: blocks without text, so even thousands
/// are cheap. Every line is `DIFF_LINE_HEIGHT` tall, so it all lines up.
pub(super) struct DiffView {
    /// The file's path and its diff as git gave it, kept to tell whether a reload
    /// changed anything and to work out the syntax colors.
    path: String,
    diff: Arc<Diff>,
    /// Whether `start_highlighting` has run.
    highlighting_started: bool,
    added: usize,
    removed: usize,
    chunks: Vec<Chunk>,
    runs: Vec<Run>,
    /// How wide the rows must be to fit the longest line, gutter and padding included.
    width: f32,
}

/// `CHUNK_LINES` consecutive diff lines (the last chunk may have fewer), one text per
/// column, with one line per diff line. One text per column rather than a row of
/// widgets per line keeps the widget count low.
struct Chunk {
    old_numbers: String,
    new_numbers: String,
    /// The sign column is two texts drawn over each other, since a text has one color:
    /// green `+` on added lines and red `−` on removed ones, each blank elsewhere.
    plus_signs: String,
    minus_signs: String,
    /// The code, in its syntax colors.
    code: ColoredText,
}

impl Chunk {
    /// `colors` has the lines' syntax colors (see `highlight::diff_colors`), or is
    /// `None` to draw them all in the default text color.
    fn new(lines: &[DiffLine], colors: Option<&[LineColors]>) -> Chunk {
        let column =
            |cell: fn(&DiffLine) -> String| lines.iter().map(cell).collect::<Vec<_>>().join("\n");
        fn number(number: Option<usize>) -> String {
            number.map(|n| n.to_string()).unwrap_or_default()
        }

        let mut code = ColoredText::default();
        for (index, line) in lines.iter().enumerate() {
            if index > 0 {
                code.push_newline();
            }
            let line_colors = colors.map_or(&[][..], |colors| colors[index].as_slice());
            code.push_line(line.text(), line_colors);
        }

        Chunk {
            old_numbers: column(|line| number(line.old_number())),
            new_numbers: column(|line| number(line.new_number())),
            plus_signs: column(|line| match line {
                DiffLine::Added { .. } => "+".to_string(),
                _ => String::new(),
            }),
            minus_signs: column(|line| match line {
                DiffLine::Removed { .. } => "−".to_string(),
                _ => String::new(),
            }),
            code,
        }
    }
}

/// Text built piece by piece, each piece in its own color, for a `rich_text`.
#[derive(Default)]
struct ColoredText {
    text: String,
    /// Byte ranges of `text`, in order and covering all of it, each with its color
    /// (`None`: the default text color).
    colors: Vec<(Range<usize>, Option<Color>)>,
}

impl ColoredText {
    /// Adds a diff line as it's drawn (see `display_text`), in its syntax colors (see
    /// `LineColors`), or in the default color if it has none.
    fn push_line(&mut self, line: &str, colors: &[(Range<usize>, Color)]) {
        let mut after_punctuation = false;
        if colors.is_empty() {
            self.push(line, None, &mut after_punctuation);
        }
        for (range, color) in colors {
            self.push(&line[range.clone()], Some(*color), &mut after_punctuation);
        }
    }

    /// Ends a line. The `\n` takes the color of the piece before it, since it isn't
    /// drawn: a piece of its own would just be one more span.
    fn push_newline(&mut self) {
        self.text.push('\n');
        match self.colors.last_mut() {
            Some((range, _)) => range.end = self.text.len(),
            None => self.colors.push((0..self.text.len(), None)),
        }
    }

    /// Adds `piece`, part of a line, in `color`. A neighbour of the same color merges
    /// with it, so there are fewer spans to draw.
    fn push(&mut self, piece: &str, color: Option<Color>, after_punctuation: &mut bool) {
        let start = self.text.len();
        push_display_text(&mut self.text, piece, after_punctuation);
        let end = self.text.len();
        match self.colors.last_mut() {
            Some((range, last)) if *last == color => range.end = end,
            _ if start < end => self.colors.push((start..end, color)),
            _ => {}
        }
    }
}

/// Consecutive diff lines of the same kind.
struct Run {
    kind: Kind,
    lines: usize,
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
    /// The diff of the file at `path`, its code in the default text color until
    /// `start_highlighting`'s colors come in.
    pub(super) fn new(path: &str, diff: Diff) -> DiffView {
        let (added, removed) = diff.counts();

        let mut runs: Vec<Run> = Vec::new();
        for kind in diff.lines.iter().map(Kind::of) {
            match runs.last_mut() {
                Some(run) if run.kind == kind => run.lines += 1,
                _ => runs.push(Run { kind, lines: 1 }),
            }
        }

        DiffView {
            path: path.to_string(),
            added,
            removed,
            chunks: chunks(&diff, None),
            runs,
            width: content_width(&diff),
            diff: Arc::new(diff),
            highlighting_started: false,
        }
    }

    /// Whether this already shows `diff` of the file at `path`. A reload that changed
    /// nothing (most refreshes) then keeps it, colors and all.
    pub(super) fn shows(&self, path: &str, diff: &Diff) -> bool {
        self.path == path && *self.diff == *diff
    }

    /// The task that works out the code's syntax colors, off the UI thread, since that
    /// can take a while (up to 0.7 s for both sides of a 5000-line file). `None` once
    /// it's been started. The result comes back as `Message::DiffHighlighted`.
    pub(super) fn start_highlighting(&mut self) -> Option<Task<Message>> {
        if self.highlighting_started {
            return None;
        }
        self.highlighting_started = true;

        // On a thread of its own. iced polls a new task once on the UI thread, before
        // handing it to its thread pool (iced_winit's `update`), so the work in an
        // `async` block that never waits would run right there and freeze the app.
        // Waiting on the channel makes that first poll return at once instead.
        let (sender, receiver) = oneshot::channel();
        let (path, diff) = (self.path.clone(), Arc::clone(&self.diff));
        thread::spawn(move || {
            // An error only means the app no longer waits for it.
            let _ = sender.send(highlight::diff_colors(&path, &diff));
        });

        let diff = Arc::clone(&self.diff);
        // If the thread panicked (its message is printed), the code stays uncolored.
        Some(Task::perform(receiver, move |colors| {
            Message::DiffHighlighted(diff, colors.ok().flatten())
        }))
    }

    /// Colors the code with what `start_highlighting` worked out for `diff`, unless
    /// that's not the diff shown any more (another file was opened, or this one changed,
    /// while it ran).
    pub(super) fn set_colors(&mut self, diff: &Arc<Diff>, colors: &[LineColors]) {
        if Arc::ptr_eq(&self.diff, diff) {
            self.chunks = chunks(&self.diff, Some(colors));
        }
    }
}

/// `diff`'s lines in `Chunk`s. `colors` has their syntax colors (see
/// `highlight::diff_colors`), or is `None` to draw the code in the default text color.
fn chunks(diff: &Diff, colors: Option<&[LineColors]>) -> Vec<Chunk> {
    let mut color_chunks = colors.map(|colors| colors.chunks(CHUNK_LINES));
    diff.lines
        .chunks(CHUNK_LINES)
        .map(|lines| Chunk::new(lines, color_chunks.as_mut().and_then(Iterator::next)))
        .collect()
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
            diff_body(diff, self.diff_scroll_y),
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
/// code, each the visible chunks' texts over a column of tint blocks (see `DiffView`).
/// The gutter stays at the left edge when scrolled sideways. `scroll_y` is how far down
/// it's scrolled, which decides the chunks that are built.
///
/// Lines don't wrap, so the code is as wide as its longest line (`DiffView::width`), or
/// the panel if that's wider, so the tints run edge to edge. (Nothing can just be `Fill`:
/// inside a both-ways scrollable that lays out empty.)
fn diff_body(diff: &DiffView, scroll_y: f32) -> Element<'_, Message> {
    responsive(move |size| {
        let tint_width = diff.width.max(size.width) - GUTTER_WIDTH;
        // A fixed width, so the text's bounds don't change when the panel is resized:
        // that would make iced lay out every line again.
        let code_width = diff.width - GUTTER_WIDTH;
        let window = ChunkWindow::new(scroll_y, size.height, diff.chunks.len());

        // In a `stack`, the first layer sets the size (here, the whole file's), and the
        // rest are drawn over it.
        let gutter = stack(
            std::iter::once(column(diff.runs.iter().map(gutter_run)).into())
                .chain(window.layers(&diff.chunks, gutter_chunk)),
        );
        let code = stack(
            std::iter::once(column(diff.runs.iter().map(|run| code_tint(run, tint_width))).into())
                .chain(window.layers(&diff.chunks, |chunk| code_text(&chunk.code, code_width))),
        );

        // The gutter goes over a gap left for it, as a `Sticky` stack layer, so it stays
        // put when the code is scrolled sideways.
        let content = stack![row![space().width(GUTTER_WIDTH), code], sticky(gutter)];

        scrollable(container(content).padding(Padding::ZERO.bottom(8.0)))
            .id(DIFF_SCROLLABLE)
            .direction(Direction::Both {
                vertical: theme::thin_scrollbar(),
                horizontal: theme::thin_scrollbar(),
            })
            .on_scroll(|viewport| Message::DiffScrolled(viewport.absolute_offset().y))
            .style(theme::scrollbar)
            .width(Fill)
            .height(Fill)
            .into()
    })
    .into()
}

/// The chunks put on screen: the ones the visible area touches, plus one more on each
/// side as a margin.
///
/// Each chunk is a `stack` layer pushed down to its place, and chunk `i` always goes in
/// layer `i % slots`. Scrolling one chunk further then changes the content of one layer
/// only: the others keep theirs, so iced keeps their shaped text and shapes just the
/// new chunk. (In a `column`, every chunk would move up one place, and iced matches
/// widgets to their previous state by place, so all of them would be shaped again.
/// `keyed_column` doesn't help: it only follows a single insert or removal.)
///
/// iced handles the scroll message and rebuilds the view before drawing the frame,
/// so the chunks are in place on the same frame the scroll shows.
#[derive(Debug, PartialEq)]
struct ChunkWindow {
    chunks: Range<usize>,
    slots: usize,
}

impl ChunkWindow {
    /// The window for a viewport `height` tall, scrolled `scroll_y` down, over `count`
    /// chunks. It's kept within the file, so a stale `scroll_y` (the file got shorter
    /// and the scrollable clamped its offset) still covers the end, where it now is.
    fn new(scroll_y: f32, height: f32, count: usize) -> ChunkWindow {
        let chunk_height = lines_height(CHUNK_LINES);
        // A viewport touches at most `ceil(height / chunk_height) + 1` chunks, and
        // there's one more on each side. Never more slots than chunks (and at least
        // one), so an unbounded `height` can't overflow or allocate endless slots.
        let slots = ((height / chunk_height).ceil() as usize)
            .saturating_add(3)
            .min(count.max(1));
        let first = ((scroll_y / chunk_height) as usize)
            .saturating_sub(1)
            .min(count.saturating_sub(slots));
        ChunkWindow {
            chunks: first..(first + slots).min(count),
            slots,
        }
    }

    /// One stack layer per slot: the chunk `draw` makes, moved down to its place, or
    /// nothing if no chunk in the window uses that slot.
    fn layers<'a>(
        &self,
        chunks: &'a [Chunk],
        draw: impl Fn(&'a Chunk) -> Element<'a, Message>,
    ) -> Vec<Element<'a, Message>> {
        let mut layers: Vec<Element<'a, Message>> =
            (0..self.slots).map(|_| space().into()).collect();
        for index in self.chunks.clone() {
            layers[index % self.slots] = container(draw(&chunks[index]))
                .padding(Padding::ZERO.top(lines_height(index * CHUNK_LINES)))
                .into();
        }
        layers
    }
}

/// The height of `lines` diff lines.
fn lines_height(lines: usize) -> f32 {
    lines as f32 * layouts::DIFF_LINE_HEIGHT
}

/// One chunk's line numbers and signs.
fn gutter_chunk(chunk: &Chunk) -> Element<'_, Message> {
    row![
        line_numbers(&chunk.old_numbers, OLD_NUMBER_PADDING),
        line_numbers(&chunk.new_numbers, NEW_NUMBER_PADDING),
        stack![
            mono(chunk.plus_signs.as_str()).color(colors::ADDED),
            mono(chunk.minus_signs.as_str()).color(colors::DELETED),
        ]
        .width(layouts::DIFF_SIGN_WIDTH),
    ]
    .into()
}

/// One run's stretch of the gutter background. It's opaque even for unchanged lines,
/// since the sticky gutter covers code scrolled under it.
fn gutter_run(run: &Run) -> Element<'_, Message> {
    let background = match run.kind {
        Kind::Context => colors::BACKGROUND,
        Kind::Added => colors::ADDED_GUTTER,
        Kind::Removed => colors::REMOVED_GUTTER,
    };

    container(space())
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

/// A chunk's code: `mono`'s text, but one span per colored piece. A `rich_text` always
/// shapes with `Shaping::Advanced`, and the text engine only shapes a span apart from
/// its neighbours when the font changes, not the color, so ligatures still form
/// across pieces (checked in cosmic-text's `ShapeWord::new` and `Attrs::compatible`).
fn code_text(code: &ColoredText, width: f32) -> Element<'_, Message> {
    let spans: Vec<Span<'_>> = code
        .colors
        .iter()
        .map(|(range, color)| span(&code.text[range.clone()]).color_maybe(*color))
        .collect();

    rich_text(spans)
        .font(fonts::MONO)
        .size(text_sizes::SMALL)
        .line_height(Pixels(layouts::DIFF_LINE_HEIGHT))
        .wrapping(Wrapping::None)
        .width(width)
        .into()
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
    let mut shown = String::with_capacity(line.len() + line.len() / 4);
    push_display_text(&mut shown, line, &mut false);
    shown
}

/// Appends `piece`, a whole line or part of one, to `shown` as `display_text` draws it.
/// `after_punctuation` says whether the line so far ends in ASCII punctuation (a
/// joiner may be due between that and `piece`), and is updated, so a line pushed in
/// pieces comes out the same as pushed whole.
fn push_display_text(shown: &mut String, piece: &str, after_punctuation: &mut bool) {
    for character in piece.chars() {
        match character {
            '\r' => {}
            '\t' => {
                shown.push_str("    ");
                *after_punctuation = false;
            }
            _ => {
                let is_punctuation = character.is_ascii_punctuation();
                if *after_punctuation && is_punctuation {
                    shown.push('\u{2060}');
                }
                shown.push(character);
                *after_punctuation = is_punctuation;
            }
        }
    }
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
        let view = DiffView::new("x", diff);

        assert_eq!((view.added, view.removed), (1, 2));
        assert_eq!(view.chunks.len(), 1);
        let chunk = &view.chunks[0];
        assert_eq!(chunk.old_numbers, "1\n2\n3\n\n4");
        assert_eq!(chunk.new_numbers, "1\n\n\n2\n3");
        assert_eq!(chunk.code.text, "same\nold\nolder\nnew\n");
        // Uncolored: one default-color span for all of it.
        assert_eq!(chunk.code.colors, vec![(0..chunk.code.text.len(), None)]);
        assert_eq!(chunk.plus_signs, "\n\n\n+\n");
        assert_eq!(chunk.minus_signs, "\n−\n−\n\n");
        let runs: Vec<_> = view.runs.iter().map(|run| run.lines).collect();
        assert_eq!(runs, vec![1, 2, 1, 1]);
    }

    #[test]
    fn test_chunks_split_every_chunk_lines() {
        let diff = Diff {
            lines: (1..=CHUNK_LINES * 2 + 1)
                .map(|n| DiffLine::Context {
                    old: n,
                    new: n,
                    text: n.to_string(),
                })
                .collect(),
        };
        let view = DiffView::new("x", diff);

        assert_eq!(view.chunks.len(), 3);
        assert!(view.chunks[1].old_numbers.starts_with("65\n"));
        assert_eq!(view.chunks[2].code.text, "129");
    }

    #[test]
    fn test_colored_lines_keep_their_joiners_and_colors() {
        let (red, blue) = (Color::from_rgb8(255, 0, 0), Color::from_rgb8(0, 0, 255));
        let mut code = ColoredText::default();
        // "\tx -> y" with "x -" red and "> y" blue: the joiner between `-` and `>`
        // goes in although they're in different pieces.
        code.push_line("\tx -> y", &[(0..4, red), (4..7, blue)]);
        code.push_newline();
        code.push_line("plain", &[]);

        assert_eq!(code.text, "    x -\u{2060}> y\nplain");
        // The same as the line pushed whole.
        assert_eq!(code.text, display_text("\tx -> y") + "\nplain");
        let blue_start = "    x -".len();
        let plain_start = code.text.len() - "plain".len();
        assert_eq!(
            code.colors,
            vec![
                (0..blue_start, Some(red)),
                // The newline joins the piece before it.
                (blue_start..plain_start, Some(blue)),
                (plain_start..code.text.len(), None),
            ]
        );
    }

    #[test]
    fn test_colors_go_to_their_chunk_and_only_for_the_shown_diff() {
        let red = Color::from_rgb8(255, 0, 0);
        let diff = Diff {
            lines: (1..=CHUNK_LINES + 1)
                .map(|n| DiffLine::Added {
                    new: n,
                    text: "ab".to_string(),
                })
                .collect(),
        };
        // Only the last line (alone in the second chunk) is colored.
        let mut colors = vec![LineColors::new(); diff.lines.len()];
        colors[CHUNK_LINES] = vec![(0..2, red)];
        let mut view = DiffView::new("x.rs", diff);

        // Colors worked out for another diff (one shown before) are dropped.
        view.set_colors(&Arc::new(Diff { lines: Vec::new() }), &colors);
        assert_eq!(view.chunks[1].code.colors, vec![(0..2, None)]);

        let shown = Arc::clone(&view.diff);
        view.set_colors(&shown, &colors);
        assert_eq!(view.chunks[0].code.colors.len(), 1);
        assert_eq!(view.chunks[0].code.colors[0].1, None);
        assert_eq!(view.chunks[1].code.colors, vec![(0..2, Some(red))]);
    }

    #[test]
    fn test_reloads_are_recognized_and_highlighting_starts_once() {
        let diff = || Diff {
            lines: vec![DiffLine::Added {
                new: 1,
                text: "fn main() {}".to_string(),
            }],
        };
        let mut view = DiffView::new("main.rs", diff());

        assert!(view.shows("main.rs", &diff()));
        assert!(!view.shows("lib.rs", &diff()));
        assert!(!view.shows("main.rs", &Diff { lines: Vec::new() }));
        assert!(view.start_highlighting().is_some());
        assert!(view.start_highlighting().is_none());
    }

    #[test]
    fn test_chunk_window_covers_the_viewport_plus_a_margin() {
        let chunk = lines_height(CHUNK_LINES);
        // A viewport under one chunk tall: up to 2 chunks touched, plus 1 each side.
        let window = |scroll_y: f32, count| ChunkWindow::new(scroll_y, chunk / 2.0, count);

        assert_eq!(
            window(0.0, 100),
            ChunkWindow {
                chunks: 0..4,
                slots: 4
            }
        );
        // Scrolled into chunk 10: from chunk 9 (the margin above) on.
        assert_eq!(window(chunk * 10.5, 100).chunks, 9..13);
        // Past the end (a stale offset): the last chunks.
        assert_eq!(window(chunk * 500.0, 100).chunks, 96..100);
        // A file shorter than the window.
        assert_eq!(window(0.0, 2).chunks, 0..2);
        assert_eq!(window(0.0, 0).chunks, 0..0);
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
