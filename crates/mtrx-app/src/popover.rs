//! A panel floating above or below a widget, over everything else, as for
//! emoji. A click outside the panel closes it and still reaches what was
//! clicked, e.g. the text field.

use iced::{
    Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector,
    advanced::{
        Clipboard, Layout, Shell, Widget,
        layout::{Limits, Node},
        overlay, renderer,
        widget::{Operation, Tree},
    },
    mouse,
    widget::space,
};

/// Space between the widget and the panel.
const GAP: f32 = 4.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Above,
    Below,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
}

pub struct Popover<'a, Message> {
    anchor: Element<'a, Message>,
    /// The panel, or a placeholder while closed (the widget tree keeps its
    /// shape either way).
    panel: Element<'a, Message>,
    open: bool,
    side: Side,
    edge: Edge,
    on_dismiss: Option<Message>,
}

/// `anchor`, with `panel` floating on `side` of it, lined up with its
/// `edge`, while given. It moves to the other side if there is no room.
pub fn popover<'a, Message: Clone + 'a>(
    anchor: impl Into<Element<'a, Message>>,
    panel: Option<Element<'a, Message>>,
    side: Side,
    edge: Edge,
) -> Popover<'a, Message> {
    Popover {
        anchor: anchor.into(),
        open: panel.is_some(),
        panel: panel.unwrap_or_else(|| space().into()),
        side,
        edge,
        on_dismiss: None,
    }
}

impl<Message> Popover<'_, Message> {
    /// Sent on a click outside the panel.
    pub fn on_dismiss(mut self, message: Message) -> Self {
        self.on_dismiss = Some(message);
        self
    }
}

impl<'a, Message: Clone + 'a> Widget<Message, Theme, Renderer> for Popover<'a, Message> {
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.anchor), Tree::new(&self.panel)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[self.anchor.as_widget(), self.panel.as_widget()]);
    }

    fn size(&self) -> Size<Length> {
        self.anchor.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.anchor.as_widget().size_hint()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &Limits) -> Node {
        self.anchor
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.anchor.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.anchor
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.anchor.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.anchor.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let [anchor_tree, panel_tree] = &mut tree.children[..] else {
            return None;
        };
        let nested = self.anchor.as_widget_mut().overlay(
            anchor_tree,
            layout,
            renderer,
            viewport,
            translation,
        );
        let panel = self.open.then(|| {
            floating(
                &mut self.panel,
                panel_tree,
                layout.bounds() + translation,
                self.side,
                self.edge,
                self.on_dismiss.clone(),
            )
        });
        let overlays: Vec<_> = nested.into_iter().chain(panel).collect();
        (!overlays.is_empty()).then(|| overlay::Group::with_children(overlays).overlay())
    }
}

/// `panel` floating on `side` of `anchor` (in window coordinates), lined
/// up with its `edge`, over everything else; a click outside it sends
/// `on_dismiss`.
pub fn floating<'a, 'b, Message: Clone + 'a>(
    panel: &'b mut Element<'a, Message>,
    tree: &'b mut Tree,
    anchor: Rectangle,
    side: Side,
    edge: Edge,
    on_dismiss: Option<Message>,
) -> overlay::Element<'b, Message, Theme, Renderer> {
    overlay::Element::new(Box::new(Panel {
        panel,
        tree,
        anchor,
        side,
        edge,
        on_dismiss,
    }))
}

struct Panel<'a, 'b, Message> {
    panel: &'b mut Element<'a, Message>,
    tree: &'b mut Tree,
    anchor: Rectangle,
    side: Side,
    edge: Edge,
    on_dismiss: Option<Message>,
}

impl<Message: Clone> overlay::Overlay<Message, Theme, Renderer> for Panel<'_, '_, Message> {
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> Node {
        let node = self.panel.as_widget_mut().layout(
            self.tree,
            renderer,
            &Limits::new(Size::ZERO, bounds),
        );
        let size = node.size();
        let x = match self.edge {
            Edge::Left => self.anchor.x,
            Edge::Right => self.anchor.x + self.anchor.width - size.width,
        }
        .clamp(0.0, (bounds.width - size.width).max(0.0));
        let above = self.anchor.y - GAP - size.height;
        let below = self.anchor.y + self.anchor.height + GAP;
        let fits_above = above >= 0.0;
        let fits_below = below + size.height <= bounds.height;
        let y = match self.side {
            Side::Above if fits_above || !fits_below => above.max(0.0),
            Side::Below if fits_below || !fits_above => below,
            Side::Above => below,
            Side::Below => above.max(0.0),
        };
        node.move_to((x, y))
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.panel.as_widget().draw(
            self.tree,
            renderer,
            theme,
            style,
            layout,
            cursor,
            &layout.bounds(),
        );
    }

    fn operate(&mut self, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
        self.panel
            .as_widget_mut()
            .operate(self.tree, layout, renderer, operation);
    }

    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        if let Event::Mouse(mouse::Event::ButtonPressed(_)) = event
            && !cursor.is_over(layout.bounds())
            && let Some(message) = &self.on_dismiss
        {
            shell.publish(message.clone());
        }
        self.panel.as_widget_mut().update(
            self.tree,
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            &layout.bounds(),
        );
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.panel.as_widget().mouse_interaction(
            self.tree,
            layout,
            cursor,
            &layout.bounds(),
            renderer,
        )
    }
}

impl<'a, Message: Clone + 'a> From<Popover<'a, Message>> for Element<'a, Message> {
    fn from(popover: Popover<'a, Message>) -> Self {
        Element::new(popover)
    }
}
