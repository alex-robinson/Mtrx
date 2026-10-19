//! Small vector icons drawn with a canvas (the text font has no such glyphs).

use std::f32::consts::FRAC_PI_2;

use iced::{
    Color, Element, Rectangle, Renderer, Theme, Vector, mouse,
    widget::canvas::{self, Frame, Geometry, Path, Stroke},
};

/// A clock face with its hands at twelve and four.
pub fn clock<'a, M: 'a>(size: f32, color: fn(&Theme) -> Color) -> Element<'a, M> {
    canvas::Canvas::new(Clock { color })
        .width(size)
        .height(size)
        .into()
}

struct Clock {
    color: fn(&Theme) -> Color,
}

impl<M> canvas::Program<M> for Clock {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let width = (bounds.width / 9.0).max(1.0);
        let center = frame.center();
        let radius = bounds.width / 2.0 - width;
        let stroke = Stroke::default()
            .with_color((self.color)(theme))
            .with_width(width)
            .with_line_cap(canvas::LineCap::Round);
        frame.stroke(&Path::circle(center, radius), stroke);
        // Angles from 12 o'clock, clockwise.
        let hand = |angle: f32, length: f32| {
            let direction = Vector::new((angle - FRAC_PI_2).cos(), (angle - FRAC_PI_2).sin());
            Path::line(center, center + direction * (radius * length))
        };
        frame.stroke(&hand(0.0, 0.7), stroke);
        frame.stroke(&hand(FRAC_PI_2 * 4.0 / 3.0, 0.5), stroke);
        vec![frame.into_geometry()]
    }
}
