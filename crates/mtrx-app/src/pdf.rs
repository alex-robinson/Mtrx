//! PDF documents: reading them and drawing their pages, for the viewer
//! (hayro: pure Rust, so nothing to ship alongside the app).

use std::sync::Arc;

use hayro::{
    RenderCache, RenderSettings, hayro_interpret::InterpreterSettings, hayro_syntax::Pdf,
    vello_cpu::color::palette::css::WHITE,
};
use iced::Size;
use mtrx_core::{Media, MediaKind};

/// PDFs larger than this are not opened in the viewer (they would be
/// downloaded just to check them).
pub const MAX_BYTES: u64 = 30_000_000;
/// Pages are drawn at most this many pixels wide or high.
const MAX_PIXELS: f32 = 4096.0;

pub fn is_pdf(media: &Media) -> bool {
    media.kind == MediaKind::File
        && (media.mimetype.as_deref() == Some("application/pdf")
            || media.filename.to_lowercase().ends_with(".pdf"))
}

/// A PDF that can be read, with its pages' sizes in points.
pub struct Document {
    pdf: Pdf,
    pub pages: Vec<Size>,
}

impl std::fmt::Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Document({} pages)", self.pages.len())
    }
}

/// A drawn page: RGBA pixels and their size.
pub struct Page {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl Document {
    /// `None` if it is not a PDF we can read, or has no pages.
    pub fn open(bytes: Vec<u8>) -> Option<Self> {
        let pdf = Pdf::new(Arc::new(bytes)).ok()?;
        let pages: Vec<Size> = pdf
            .pages()
            .iter()
            .map(|page| {
                let (width, height) = page.render_dimensions();
                Size::new(width, height)
            })
            .collect();
        if pages.is_empty() || pages.iter().any(|p| p.width <= 0.0 || p.height <= 0.0) {
            return None;
        }
        Some(Self { pdf, pages })
    }

    /// Page `index` drawn on white at `scale` pixels per point (less if
    /// that would be too large).
    pub fn render(&self, index: usize, scale: f32) -> Option<Page> {
        let page = self.pdf.pages().get(index)?;
        let size = self.pages[index];
        let scale = scale.min(MAX_PIXELS / size.width.max(size.height));
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
        Some(Page {
            rgba,
            width,
            height,
        })
    }
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
    fn document_is_shareable() {
        fn check<T: Send + Sync>() {}
        check::<Document>();
    }

    #[test]
    fn reads_and_draws_pages() {
        let document = Document::open(sample()).unwrap();
        assert_eq!(document.pages, [Size::new(300.0, 400.0); 2]);
        let page = document.render(1, 2.0).unwrap();
        assert_eq!((page.width, page.height), (600, 800));
        assert_eq!(page.rgba.len(), 600 * 800 * 4);
        // White margin, the blue box in the middle.
        let pixel = |x: u32, y: u32| {
            let i = ((y * page.width + x) * 4) as usize;
            &page.rgba[i..i + 4]
        };
        assert_eq!(pixel(4, 4), [255, 255, 255, 255]);
        assert_eq!(pixel(300, 320), [51, 102, 204, 255]);
        // Huge scales are capped.
        assert_eq!(document.render(0, 100.0).unwrap().height, 4096);
        assert!(document.render(2, 1.0).is_none());
        assert!(Document::open(b"not a pdf".to_vec()).is_none());
    }
}
