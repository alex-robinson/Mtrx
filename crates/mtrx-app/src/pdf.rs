//! Pages of PDF files drawn as pictures, for previews and the viewer
//! (hayro: pure Rust, so nothing to ship alongside the app).

use std::sync::Arc;

use hayro::{
    RenderCache, RenderSettings, hayro_interpret::InterpreterSettings, hayro_syntax::Pdf,
    vello_cpu::color::palette::css::WHITE,
};
use mtrx_core::{Media, MediaKind};

/// PDFs larger than this are not downloaded for a preview.
pub const MAX_PREVIEW_BYTES: u64 = 30_000_000;

pub fn is_pdf(media: &Media) -> bool {
    media.kind == MediaKind::File
        && (media.mimetype.as_deref() == Some("application/pdf")
            || media.filename.to_lowercase().ends_with(".pdf"))
}

/// A rendered page: RGBA pixels and their size.
pub struct Page {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// The number of pages, and page `index` drawn to fit `max_width`×
/// `max_height` pixels on white; `None` if it is not a PDF we can read.
pub fn render(
    bytes: Arc<Vec<u8>>,
    index: usize,
    max_width: u32,
    max_height: u32,
) -> Option<(usize, Page)> {
    let pdf = Pdf::new(bytes).ok()?;
    let pages = pdf.pages();
    let page = pages.get(index)?;
    let (width, height) = page.render_dimensions();
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    let scale = (max_width as f32 / width).min(max_height as f32 / height);
    let settings = RenderSettings {
        x_scale: scale,
        y_scale: scale,
        bg_color: WHITE,
        ..Default::default()
    };
    let pixmap = hayro::render(
        page,
        &RenderCache::new(),
        &InterpreterSettings::default(),
        &settings,
    );
    let (width, height) = (u32::from(pixmap.width()), u32::from(pixmap.height()));
    // Opaque on white, so premultiplied and plain RGBA are the same.
    let rgba = pixmap.data_as_u8_slice().to_vec();
    Some((
        pages.len(),
        Page {
            rgba,
            width,
            height,
        },
    ))
}

/// A two-page PDF with a title and a blue box, 300×400 points (tests).
#[cfg(test)]
pub fn sample() -> Vec<u8> {
    let page = |title: &str| {
        format!(
            "0.2 0.4 0.8 rg 40 160 220 150 re f BT /F1 24 Tf 0 0 0 rg 40 340 Td ({title}) Tj ET"
        )
    };
    let contents = [page("Ice flux draft"), page("Results")];
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [4 0 R 5 0 R] /Count 2 >>".to_owned(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
    ];
    for i in 0..2 {
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] \
             /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
            6 + i
        ));
    }
    for content in &contents {
        objects.push(format!(
            "<< /Length {} >>\nstream\n{content}\nendstream",
            content.len()
        ));
    }
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend(format!("{} 0 obj\n{object}\nendobj\n", i + 1).bytes());
    }
    let xref = pdf.len();
    pdf.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).bytes());
    for offset in offsets {
        pdf.extend(format!("{offset:010} 00000 n \n").bytes());
    }
    pdf.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .bytes(),
    );
    pdf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_a_page_to_fit() {
        let (pages, page) = render(Arc::new(sample()), 1, 300, 600).unwrap();
        assert_eq!(pages, 2);
        assert_eq!((page.width, page.height), (300, 400));
        assert_eq!(page.rgba.len(), 300 * 400 * 4);
        // White margin, the blue box in the middle.
        let pixel = |x: u32, y: u32| {
            let i = ((y * page.width + x) * 4) as usize;
            &page.rgba[i..i + 4]
        };
        assert_eq!(pixel(2, 2), [255, 255, 255, 255]);
        assert_eq!(pixel(150, 160), [51, 102, 204, 255]);
        assert!(render(Arc::new(sample()), 2, 300, 600).is_none());
        assert!(render(Arc::new(b"not a pdf".to_vec()), 0, 300, 600).is_none());
    }
}
