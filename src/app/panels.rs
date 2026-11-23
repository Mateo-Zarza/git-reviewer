//! Draggable panel widths, like VS Code's sashes: the sidebar and the changes panel each
//! get the width the user drags the divider on their right to, and the diff takes the
//! rest. Widths are logical pixels. The panels start at the window's left edge, so a
//! divider's position is simply the cursor's x.

use iced::event::{self, Status};
use iced::widget::{mouse_area, opaque, row, space};
use iced::{Element, Event, Fill, Subscription, mouse, window};
use serde::{Deserialize, Serialize};

use super::{Message, changes};
use crate::theme::layouts;

/// The divider on the right of a panel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Divider {
    Sidebar,
    Changes,
}

/// The widths the user chose. The window may be too narrow to show them in full; then
/// `fitted` gives what's shown, and these stay untouched for when it grows again.
/// Saved in `state.toml`; a missing key there gives that panel's default. Whatever a
/// hand-edited one says is still clamped by `fitted`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PanelWidths {
    pub sidebar: f32,
    pub changes: f32,
}

impl Default for PanelWidths {
    fn default() -> Self {
        PanelWidths {
            sidebar: layouts::SIDEBAR_DEFAULT_WIDTH,
            changes: layouts::CHANGES_DEFAULT_WIDTH,
        }
    }
}

impl PanelWidths {
    /// The widths that fit in a window `available` wide while leaving the diff its
    /// minimum. The diff gives way first (it's `Fill`), then the changes panel, then the
    /// sidebar, each down to its minimum. Below the sum of the minimums nothing shrinks
    /// further; the window's own minimum size keeps it from getting there.
    pub fn fitted(self, available: f32) -> PanelWidths {
        let room = side_room(available);
        let changes = self
            .changes
            .min(room - self.sidebar)
            .max(changes::min_width());
        let sidebar = self
            .sidebar
            .min(room - changes)
            .max(layouts::SIDEBAR_MIN_WIDTH);

        PanelWidths { sidebar, changes }
    }

    /// The widths after dragging `divider` to `x`, where `self` is the widths when the
    /// drag started. Like VS Code, only the two panels beside the divider change, and
    /// each move is worked out from the start, so dragging back undoes a push:
    /// - the sidebar's divider trades width with the changes panel, so the divider on
    ///   the changes panel's right stays put until that panel is at its minimum; then
    ///   it's pushed along, into the diff;
    /// - the changes panel's divider trades width with the diff going right; going
    ///   left, it squeezes the changes panel to its minimum, then pushes the sidebar's
    ///   divider along.
    ///
    /// Either stops when the panels it squeezes are all at their minimums.
    pub fn dragged(self, divider: Divider, x: f32, available: f32) -> PanelWidths {
        let start = self.fitted(available);
        let room = side_room(available);

        // `min` then `max`, not `clamp`: `clamp` panics when the window is so narrow
        // that the upper bound is below the minimum.
        match divider {
            Divider::Sidebar => {
                let sidebar = x
                    .min(room - changes::min_width())
                    .max(layouts::SIDEBAR_MIN_WIDTH);
                let changes = (start.sidebar + start.changes - sidebar).max(changes::min_width());
                PanelWidths { sidebar, changes }
            }
            Divider::Changes => {
                // Both panels together reach the divider, less the sidebar's own.
                let both = (x - layouts::DIVIDER_WIDTH)
                    .min(room)
                    .max(layouts::SIDEBAR_MIN_WIDTH + changes::min_width());
                let sidebar = start.sidebar.min(both - changes::min_width());
                PanelWidths {
                    sidebar,
                    changes: both - sidebar,
                }
            }
        }
    }
}

/// What the sidebar and changes panel may take together in a window `available` wide:
/// everything but the diff's minimum and the two dividers.
fn side_room(available: f32) -> f32 {
    available - layouts::DIFF_MIN_WIDTH - 2.0 * layouts::DIVIDER_WIDTH
}

/// The window's smallest width: every panel at its minimum, plus the dividers.
pub fn min_window_width() -> f32 {
    layouts::SIDEBAR_MIN_WIDTH
        + changes::min_width()
        + layouts::DIFF_MIN_WIDTH
        + 2.0 * layouts::DIVIDER_WIDTH
}

/// The grab areas, laid over the panels in a `stack`: transparent space with a strip
/// centered on each 1px divider. The strip is wider than the line, so it's easy to hit,
/// and it doesn't move the panels or cover their backgrounds.
pub fn handles<'a>(widths: PanelWidths) -> Element<'a, Message> {
    // How far a strip reaches past each side of its line.
    let overhang = (layouts::DIVIDER_HANDLE_WIDTH - layouts::DIVIDER_WIDTH) / 2.0;

    row![
        space().width(widths.sidebar - overhang),
        handle(Divider::Sidebar),
        space().width(widths.changes - 2.0 * overhang),
        handle(Divider::Changes),
    ]
    .height(Fill)
    .into()
}

fn handle<'a>(divider: Divider) -> Element<'a, Message> {
    mouse_area(space().width(layouts::DIVIDER_HANDLE_WIDTH).height(Fill))
        .on_press(Message::DividerGrabbed(divider))
        .interaction(mouse::Interaction::ResizingHorizontally)
        .into()
}

/// Laid over the whole window while a divider is dragged: it keeps the resize cursor
/// wherever the cursor goes, and stops the rows underneath from lighting up or taking
/// clicks, as in VS Code.
pub fn drag_overlay<'a>() -> Element<'a, Message> {
    opaque(
        mouse_area(space().width(Fill).height(Fill))
            .interaction(mouse::Interaction::ResizingHorizontally),
    )
}

/// The window's width, when it opens and whenever it's resized.
pub fn window_width() -> Subscription<Message> {
    event::listen_with(|event, _status, _window| match event {
        Event::Window(window::Event::Opened { size, .. } | window::Event::Resized(size)) => {
            Some(Message::WindowResized(size.width))
        }
        _ => None,
    })
}

/// The cursor while a divider is dragged. Needed because a `mouse_area` only reports
/// moves and releases over itself, and the cursor soon runs ahead of a 5px strip. Every
/// event counts, even one a widget captured (the overlay captures them all).
pub fn drag_events() -> Subscription<Message> {
    event::listen_with(|event, _status: Status, _window| match event {
        Event::Mouse(mouse::Event::CursorMoved { position }) => {
            Some(Message::DividerDragged(position.x))
        }
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
            Some(Message::DividerReleased)
        }
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDE: f32 = 2000.0;

    #[test]
    fn test_fitted_keeps_widths_that_fit() {
        let widths = PanelWidths::default();
        assert_eq!(widths.fitted(WIDE), widths);
    }

    #[test]
    fn test_fitted_shrinks_changes_first_then_sidebar() {
        let widths = PanelWidths {
            sidebar: 400.0,
            changes: 400.0,
        };
        // 50px short of fitting: only the changes panel gives way.
        let fitted = widths.fitted(400.0 + 400.0 + layouts::DIFF_MIN_WIDTH + 2.0 - 50.0);
        assert_eq!(fitted.sidebar, 400.0);
        assert_eq!(fitted.changes, 350.0);

        // At the window's minimum: both at their minimums.
        let fitted = widths.fitted(min_window_width());
        assert_eq!(fitted.sidebar, layouts::SIDEBAR_MIN_WIDTH);
        assert_eq!(fitted.changes, changes::min_width());
    }

    #[test]
    fn test_fitted_never_goes_below_minimums() {
        let fitted = PanelWidths::default().fitted(100.0);
        assert_eq!(fitted.sidebar, layouts::SIDEBAR_MIN_WIDTH);
        assert_eq!(fitted.changes, changes::min_width());
    }

    #[test]
    fn test_drag_sidebar_keeps_the_changes_divider_in_place() {
        let start = PanelWidths {
            sidebar: 260.0,
            changes: 400.0,
        };
        // The changes panel takes the difference: its right edge doesn't move.
        let dragged = start.dragged(Divider::Sidebar, 300.0, WIDE);
        assert_eq!(dragged.sidebar, 300.0);
        assert_eq!(dragged.sidebar + dragged.changes, 660.0);

        // Stops at the sidebar's minimum; the changes panel grows by as much.
        let dragged = start.dragged(Divider::Sidebar, 10.0, WIDE);
        assert_eq!(dragged.sidebar, layouts::SIDEBAR_MIN_WIDTH);
        assert_eq!(dragged.sidebar + dragged.changes, 660.0);
    }

    #[test]
    fn test_drag_sidebar_pushes_a_squeezed_changes_panel() {
        let start = PanelWidths {
            sidebar: 260.0,
            changes: 400.0,
        };
        // Past the changes panel's minimum, the panel is pushed along into the diff.
        let dragged = start.dragged(Divider::Sidebar, 600.0, WIDE);
        assert_eq!(dragged.sidebar, 600.0);
        assert_eq!(dragged.changes, changes::min_width());

        // Never past the diff's minimum.
        let dragged = start.dragged(Divider::Sidebar, WIDE, WIDE);
        assert_eq!(
            dragged.sidebar + dragged.changes,
            WIDE - layouts::DIFF_MIN_WIDTH - 2.0
        );

        // Worked out from the start, so dragging back undoes the push.
        assert_eq!(start.dragged(Divider::Sidebar, 260.0, WIDE), start);
    }

    #[test]
    fn test_drag_changes_left_pushes_the_sidebar_divider() {
        let start = PanelWidths {
            sidebar: 400.0,
            changes: 400.0,
        };
        let min = changes::min_width();

        // Squeezed to its minimum, the changes panel pushes the sidebar's divider.
        let x = 300.0 + 1.0 + min;
        let dragged = start.dragged(Divider::Changes, x, WIDE);
        assert_eq!(dragged.changes, min);
        assert_eq!(dragged.sidebar, 300.0);

        // Until the sidebar is at its minimum too.
        let dragged = start.dragged(Divider::Changes, 0.0, WIDE);
        assert_eq!(dragged.sidebar, layouts::SIDEBAR_MIN_WIDTH);
        assert_eq!(dragged.changes, min);

        // Worked out from the start, so dragging back undoes the push.
        let x = 400.0 + 1.0 + 400.0;
        assert_eq!(start.dragged(Divider::Changes, x, WIDE), start);
    }

    #[test]
    fn test_drag_changes_right_stops_at_the_diff_minimum() {
        let dragged = PanelWidths::default().dragged(Divider::Changes, WIDE, WIDE);
        assert_eq!(dragged.sidebar, PanelWidths::default().sidebar);
        assert_eq!(
            dragged.sidebar + dragged.changes,
            WIDE - layouts::DIFF_MIN_WIDTH - 2.0
        );
    }

    #[test]
    fn test_drag_changes_is_relative_to_the_sidebar() {
        let widths = PanelWidths::default();
        // The changes panel starts after the sidebar and its divider.
        let x = widths.sidebar + 1.0 + 350.0;
        assert_eq!(widths.dragged(Divider::Changes, x, WIDE).changes, 350.0);
        assert_eq!(
            widths.dragged(Divider::Changes, 0.0, WIDE).changes,
            changes::min_width()
        );
    }
}
