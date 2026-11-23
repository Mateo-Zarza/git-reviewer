//! A widget that stays at the left edge of a scrollable when it's scrolled sideways,
//! like the diff's gutter (line numbers and +/− signs) in VS Code. iced 0.14 has no
//! such widget: `float` comes closest, but once moved it draws as an overlay, above
//! everything (the error toast included) and outside the scrollable's clip.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Tree, Widget};
use iced::{Element, Length, Rectangle, Size, Vector, mouse};

/// Draws `content` shifted right by however far its scrollable is scrolled sideways, so
/// it stays at the visible left edge. It's laid out where it is, so it still scrolls
/// vertically with everything else; only the drawing moves.
///
/// Two things are needed for it to be drawn over what scrolls under it:
/// - It must be an upper layer of a `stack` (`stack![content, sticky(gutter)]`), not a
///   child of a `row` or `column`: those skip children whose layout is outside the
///   visible area, and once scrolled, the sticky part's layout is.
/// - It draws in a render layer of its own. Within one layer iced draws every
///   background before any text, so the code's text would show through its background.
///
/// It only draws: events never reach `content`, so nothing inside it can be clicked.
pub(super) struct Sticky<'a, Message> {
    content: Element<'a, Message>,
}

pub(super) fn sticky<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    Element::new(Sticky {
        content: content.into(),
    })
}

impl<Message> Widget<Message, iced::Theme, iced::Renderer> for Sticky<'_, Message> {
    // The tree, size and layout are all the content's own: `Sticky` adds no state and
    // no space.
    fn tag(&self) -> widget::tree::Tag {
        self.content.as_widget().tag()
    }

    fn state(&self) -> widget::tree::State {
        self.content.as_widget().state()
    }

    fn children(&self) -> Vec<Tree> {
        self.content.as_widget().children()
    }

    fn diff(&self, tree: &mut Tree) {
        self.content.as_widget().diff(tree);
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content.as_widget_mut().layout(tree, renderer, limits)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;

        // Inside a scrollable, `viewport` is the visible area in the content's own
        // coordinates, so its left edge minus ours is how far it's scrolled past us.
        let shift = (viewport.x - layout.bounds().x).max(0.0);

        renderer.with_layer(*viewport, |renderer| {
            renderer.with_translation(Vector::new(shift, 0.0), |renderer| {
                // Shifted right, the content sees the visible area as shifted left.
                let viewport = Rectangle {
                    x: viewport.x - shift,
                    ..*viewport
                };
                // No cursor: it's drawn away from where it's laid out, so hover checks
                // against its layout would be wrong, and it only draws anyway.
                self.content.as_widget().draw(
                    tree,
                    renderer,
                    theme,
                    style,
                    layout,
                    mouse::Cursor::Unavailable,
                    &viewport,
                );
            });
        });
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(tree, layout, renderer, operation);
    }
}
