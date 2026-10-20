//! Hover help: a short label over a control, so people find their way
//! without having to guess what a symbol does.

use iced::{
    Element,
    widget::{container, text, tooltip},
};

pub use iced::widget::tooltip::Position;

pub fn hint<'a, M: 'a>(
    content: impl Into<Element<'a, M>>,
    label: impl text::IntoFragment<'a>,
    position: Position,
) -> Element<'a, M> {
    tooltip(
        content,
        container(text(label).size(12))
            .padding(6)
            .style(container::rounded_box),
        position,
    )
    .into()
}
