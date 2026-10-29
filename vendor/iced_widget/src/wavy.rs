// Mtrx patch (spelling): the line under a misspelled word.
//! Draw a wavy line under some text.
use crate::core::renderer;
use crate::core::{Color, Rectangle};

/// How far the line rises and falls.
const AMPLITUDE: f32 = 1.5;
/// The length of one rise and fall.
const PERIOD: f32 = 4.0;
/// The length of each step drawn.
const STEP: f32 = 0.5;
/// The thickness of the line.
const THICKNESS: f32 = 1.0;

/// Draws a wavy line under the text in `run`, a rectangle from the text's
/// baseline down, clipped to `clip`.
pub(crate) fn draw<Renderer: renderer::Renderer>(
    renderer: &mut Renderer,
    run: Rectangle,
    color: Color,
    clip: Rectangle,
) {
    let top = run.y + 1.0;
    let steps = (run.width / STEP).ceil() as usize;

    for step in 0..steps {
        let x = run.x + step as f32 * STEP;
        let phase = (x - run.x) / PERIOD % 1.0;
        let y = top + AMPLITUDE * (2.0 * phase - 1.0).abs();

        let dot = Rectangle {
            x,
            y,
            width: STEP,
            height: THICKNESS,
        };

        if let Some(dot) = clip.intersection(&dot) {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: dot,
                    ..renderer::Quad::default()
                },
                color,
            );
        }
    }
}
