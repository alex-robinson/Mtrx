//! A menu opened by right-clicking a widget, floating at the cursor over
//! everything else, as for a message's menu.

use iced::{
    Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, Vector,
    advanced::{
        Clipboard, Layout, Shell, Widget,
        layout::{Limits, Node},
        overlay, renderer,
        widget::{Operation, Tree, tree},
    },
    mouse,
    widget::space,
};

use crate::popover::{Edge, Side, floating};

pub struct ContextMenu<'a, Message> {
    content: Element<'a, Message>,
    /// The menu, or a placeholder while closed (the widget tree keeps its
    /// shape either way).
    menu: Element<'a, Message>,
    open: bool,
    on_open: Message,
    on_dismiss: Option<Message>,
}

/// `content`, sending `on_open` when right-clicked; while `menu` is given,
/// it floats where that click was.
pub fn context_menu<'a, Message: Clone + 'a>(
    content: impl Into<Element<'a, Message>>,
    menu: Option<Element<'a, Message>>,
    on_open: Message,
) -> ContextMenu<'a, Message> {
    ContextMenu {
        content: content.into(),
        open: menu.is_some(),
        menu: menu.unwrap_or_else(|| space().into()),
        on_open,
        on_dismiss: None,
    }
}

impl<Message> ContextMenu<'_, Message> {
    /// Sent on a click outside the menu.
    pub fn on_dismiss(mut self, message: Message) -> Self {
        self.on_dismiss = Some(message);
        self
    }
}

/// Where the last right-click was, relative to the content.
#[derive(Default)]
struct State {
    at: Point,
}

impl<'a, Message: Clone + 'a> Widget<Message, Theme, Renderer> for ContextMenu<'a, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content), Tree::new(&self.menu)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[self.content.as_widget(), self.menu.as_widget()]);
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &Limits) -> Node {
        self.content
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
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        if shell.is_event_captured() {
            return;
        }
        if let Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) = event
            && let Some(at) = cursor.position_in(layout.bounds())
        {
            tree.state.downcast_mut::<State>().at = at;
            shell.publish(self.on_open.clone());
            shell.capture_event();
        }
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
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
        self.content.as_widget().mouse_interaction(
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
        self.content.as_widget().draw(
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
        let at = tree.state.downcast_ref::<State>().at;
        let [content_tree, menu_tree] = &mut tree.children[..] else {
            return None;
        };
        let nested = self.content.as_widget_mut().overlay(
            content_tree,
            layout,
            renderer,
            viewport,
            translation,
        );
        let menu = self.open.then(|| {
            // A point, so the menu opens to the right of and below the cursor.
            let anchor = Rectangle::new(
                layout.bounds().position() + Vector::new(at.x, at.y) + translation,
                Size::ZERO,
            );
            floating(
                &mut self.menu,
                menu_tree,
                anchor,
                Side::Below,
                Edge::Left,
                self.on_dismiss.clone(),
            )
        });
        let overlays: Vec<_> = nested.into_iter().chain(menu).collect();
        (!overlays.is_empty()).then(|| overlay::Group::with_children(overlays).overlay())
    }
}

impl<'a, Message: Clone + 'a> From<ContextMenu<'a, Message>> for Element<'a, Message> {
    fn from(menu: ContextMenu<'a, Message>) -> Self {
        Element::new(menu)
    }
}
