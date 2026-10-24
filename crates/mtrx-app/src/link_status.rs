//! The address of the link under the pointer, in the window's bottom left
//! corner, as a browser shows it. A widget with links returns it as its
//! overlay while one is hovered, so it goes when the pointer leaves.

use iced::{
    Element, Renderer, Size, Theme,
    advanced::{
        Layout,
        layout::{Limits, Node},
        overlay, renderer,
        widget::{Operation, Tree},
    },
    mouse,
    widget::container,
};

use crate::{ellipsis::ellipsis, style};

/// Text size of the address.
const SIZE: f32 = 11.0;
/// The share of the window's width the address may take before it ends in
/// "…".
const MAX_SHARE: f32 = 0.5;

/// What a widget with links keeps for showing an address.
#[derive(Default)]
pub struct Status {
    tree: Option<Tree>,
}

/// `url` in the window's corner.
pub fn overlay<'b, Message: 'b>(
    url: &'b str,
    status: &'b mut Status,
) -> overlay::Element<'b, Message, Theme, Renderer> {
    let label: Element<'b, Message> = container(ellipsis(url).size(SIZE))
        .padding([2, 6])
        .style(style::link_status)
        .into();
    let tree = status.tree.get_or_insert_with(|| Tree::new(&label));
    tree.diff(&label);
    overlay::Element::new(Box::new(Corner { url, label, tree }))
}

struct Corner<'b, Message> {
    url: &'b str,
    label: Element<'b, Message>,
    tree: &'b mut Tree,
}

impl<Message> overlay::Overlay<Message, Theme, Renderer> for Corner<'_, Message> {
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> Node {
        let max = Size::new(bounds.width * MAX_SHARE, bounds.height);
        let node =
            self.label
                .as_widget_mut()
                .layout(self.tree, renderer, &Limits::new(Size::ZERO, max));
        let height = node.size().height;
        node.move_to((0.0, bounds.height - height))
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.label.as_widget().draw(
            self.tree,
            renderer,
            theme,
            style,
            layout,
            cursor,
            &layout.bounds(),
        );
    }

    /// The whole address, also when it is shown shortened.
    fn operate(&mut self, layout: Layout<'_>, _renderer: &Renderer, operation: &mut dyn Operation) {
        operation.text(None, layout.bounds(), self.url);
    }
}
