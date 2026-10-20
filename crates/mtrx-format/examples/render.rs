//! Render a Markdown message to a PNG for visual checks:
//! `cargo run -p mtrx-format --features render --example render -- out.png 'text with $x^2$'`

use mtrx_format::{Document, RenderStyle, Renderer, markdown_to_matrix};

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().expect("output path");
    let text = args.next().expect("markdown text");
    let formatted = markdown_to_matrix(&text);
    let doc = formatted
        .html
        .map(|h| Document::from_html(&h))
        .unwrap_or_else(|| Document::from_plain(&formatted.body));
    let style = RenderStyle {
        font_family: "Libertinus Serif".into(),
        font_size: 15.0,
        max_width: 420.0,
        scale: 2.0,
        text_color: [25, 25, 25, 255],
        link_color: [30, 90, 200, 255],
        muted_color: [150, 150, 150, 255],
    };
    let image = Renderer::new::<Vec<u8>>([])
        .render(&doc, &style)
        .expect("render");
    let file = std::fs::File::create(&out).expect("create");
    let mut encoder = png::Encoder::new(file, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&image.rgba)
        .unwrap();
    println!("{out}: {}x{}", image.width, image.height);
}
