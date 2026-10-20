//! Typeset a [`Document`] with Typst into an RGBA image. LaTeX math is
//! converted to Typst math with `mitex`. The image comes with where each
//! piece of the message's text is drawn, for selecting it.

use std::{collections::HashMap, fmt::Write, ops::Range, sync::LazyLock};

use mitex::{CommandSpec, CommandSpecItem};

use typst::{
    Library, LibraryExt, World,
    diag::{FileError, FileResult},
    foundations::{Bytes, Datetime, Duration, Value},
    introspection::{MetadataElem, Tag},
    layout::{Frame, FrameItem, Point},
    syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot},
    text::{Font, FontBook},
    utils::LazyHash,
};
use typst_layout::PagedDocument;

use crate::{Block, Document, Inline, Style};

/// Typst definitions used by mitex output, served as project files.
const MITEX_FILES: &[(&str, &str)] = &[
    ("/mitex/mod.typ", include_str!("../typst/mitex/mod.typ")),
    (
        "/mitex/prelude.typ",
        include_str!("../typst/mitex/prelude.typ"),
    ),
    (
        "/mitex/latex/standard.typ",
        include_str!("../typst/mitex/latex/standard.typ"),
    ),
];
const MAIN: &str = "/main.typ";
/// Vertical page margin for content outside the line boxes, in em.
const OVERFLOW_EM: f32 = 2.0;
const MATH_FONT: &str = "New Computer Modern Math";
const MONO_FONT: &str = "DejaVu Sans Mono";

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("LaTeX `{tex}`: {message}")]
    Latex { tex: String, message: String },
    #[error("typesetting failed: {0}")]
    Typst(String),
}

/// How to typeset. Lengths are in logical pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderStyle {
    /// Text font family; it must be among the fonts given to [`Renderer::new`].
    pub font_family: String,
    pub font_size: f32,
    /// Lines wrap at this width; narrower content gives a narrower image.
    pub max_width: f32,
    /// Device pixels per logical pixel.
    pub scale: f32,
    pub text_color: [u8; 4],
    pub link_color: [u8; 4],
    /// Quote bars, spoilers and code backgrounds.
    pub muted_color: [u8; 4],
}

/// Straight (not premultiplied) RGBA pixels, and where the text is.
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// The drawn pieces of text, in reading order.
    pub pieces: Vec<Piece>,
}

/// Where a piece of a message's text is drawn in its image.
///
/// Text is counted in leaves: each text, inline code and inline math, and
/// each code block and displayed math, in reading order.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub leaf: usize,
    /// Its bytes in the leaf's text; `None` where the leaf is only selected
    /// whole (math and code), drawn as one piece per line.
    pub range: Option<Range<usize>>,
    /// Left, top, width and height, in image pixels.
    pub rect: [f32; 4],
}

/// Holds fonts and the Typst standard library; create once and reuse.
pub struct Renderer {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    files: HashMap<FileId, Source>,
    main_id: FileId,
}

impl Renderer {
    /// `text_fonts` are font files (TTF/OTF) for message text. Math and
    /// monospace fonts are built in; the system emoji font is used as
    /// fallback when found.
    pub fn new<F>(text_fonts: impl IntoIterator<Item = F>) -> Self
    where
        F: AsRef<[u8]> + Send + Sync + 'static,
    {
        let fonts: Vec<Font> = typst_assets::fonts()
            .map(Bytes::new)
            .chain(text_fonts.into_iter().map(Bytes::new))
            .chain(system_emoji_font())
            .flat_map(Font::iter)
            .collect();
        let files = MITEX_FILES
            .iter()
            .map(|(path, text)| {
                let id = file_id(path);
                (id, Source::new(id, (*text).to_owned()))
            })
            .collect();
        Self {
            library: LazyHash::new(Library::default()),
            book: LazyHash::new(FontBook::from_fonts(&fonts)),
            fonts,
            files,
            main_id: file_id(MAIN),
        }
    }

    pub fn render(&self, document: &Document, style: &RenderStyle) -> Result<Image, RenderError> {
        let mut leaves = Vec::new();
        let source = Source::new(self.main_id, typst_source(document, style, &mut leaves)?);
        let world = MessageWorld {
            renderer: self,
            main: source,
        };
        let compiled = typst::compile::<PagedDocument>(&world)
            .output
            .map_err(|errors| {
                RenderError::Typst(
                    errors
                        .iter()
                        .map(|e| e.message.to_string())
                        .collect::<Vec<_>>()
                        .join("; "),
                )
            })?;
        let page = compiled
            .pages()
            .first()
            .ok_or_else(|| RenderError::Typst("no page".into()))?;
        let pixmap = typst_render::render(
            page,
            &typst_render::RenderOptions {
                pixel_per_pt: f64::from(style.scale).into(),
                render_bleed: false,
            },
        );
        let width = pixmap.width() as usize;
        let rgba: Vec<u8> = pixmap
            .pixels()
            .iter()
            .flat_map(|p| {
                let c = p.demultiply();
                [c.red(), c.green(), c.blue(), c.alpha()]
            })
            .collect();
        let (mut image, top) = trim_rows(rgba, width, (style.scale * 2.0) as usize);
        let mut pieces = Vec::new();
        collect_pieces(&page.frame, Point::zero(), &leaves, &mut None, &mut pieces);
        let scale = f64::from(style.scale);
        image.pieces = pieces
            .into_iter()
            .map(|(leaf, range, [x, y, w, h])| Piece {
                leaf,
                range,
                rect: [
                    (x * scale) as f32,
                    (y * scale) as f32 - top as f32,
                    (w * scale) as f32,
                    (h * scale) as f32,
                ],
            })
            .collect();
        Ok(image)
    }
}

/// A piece in points: leaf, its range, and its rectangle.
type RawPiece = (usize, Option<Range<usize>>, [f64; 4]);

/// The pieces of text in `frame` (placed at `at`), between the markers that
/// [`marked`] put around each leaf. `leaves` says which are selected whole;
/// `current` is the leaf whose text is being laid out.
fn collect_pieces(
    frame: &Frame,
    at: Point,
    leaves: &[bool],
    current: &mut Option<usize>,
    out: &mut Vec<RawPiece>,
) {
    for (pos, item) in frame.items() {
        let pos = at + *pos;
        match item {
            FrameItem::Group(group) => {
                let shift = Point::new(group.transform.tx, group.transform.ty);
                collect_pieces(&group.frame, pos + shift, leaves, current, out);
            }
            FrameItem::Tag(Tag::Start(content, _)) => {
                if let Some(marker) = content.to_packed::<MetadataElem>()
                    && let Value::Int(n) = marker.value
                    && let Ok(n) = usize::try_from(n)
                {
                    *current = (n % 2 == 0).then_some(n / 2);
                }
            }
            FrameItem::Text(text) => {
                let Some(leaf) = *current else {
                    continue;
                };
                let metrics = text.font.metrics();
                let top = (pos.y - metrics.ascender.at(text.size)).to_pt();
                let height = (metrics.ascender - metrics.descender).at(text.size).to_pt();
                let mut x = pos.x;
                let mut whole: Option<[f64; 4]> = None;
                for glyph in &text.glyphs {
                    let left = (x + glyph.x_offset.at(text.size)).to_pt();
                    let width = glyph.x_advance.at(text.size).to_pt();
                    x += glyph.x_advance.at(text.size);
                    if leaves.get(leaf).copied().unwrap_or(true) {
                        let rect = [left, top, width, height];
                        whole = Some(whole.map_or(rect, |whole| union(&whole, &rect)));
                    } else {
                        let start = usize::from(glyph.span.1);
                        let range = start..start + glyph.range().len();
                        out.push((leaf, Some(range), [left, top, width, height]));
                    }
                }
                // Runs of the same whole leaf on one line (a formula's
                // symbols, accents, numerators) make one piece.
                if let Some(rect) = whole {
                    match out.last_mut() {
                        Some((last, None, previous))
                            if *last == leaf && overlap_vertically(previous, &rect) =>
                        {
                            *previous = union(previous, &rect);
                        }
                        _ => out.push((leaf, None, rect)),
                    }
                }
            }
            _ => {}
        }
    }
}

/// Drop fully transparent rows above and below the content, keeping `pad`.
fn union([l1, t1, w1, h1]: &[f64; 4], [l2, t2, w2, h2]: &[f64; 4]) -> [f64; 4] {
    let (left, top) = (l1.min(*l2), t1.min(*t2));
    let (right, bottom) = ((l1 + w1).max(l2 + w2), (t1 + h1).max(t2 + h2));
    [left, top, right - left, bottom - top]
}

fn overlap_vertically([_, t1, _, h1]: &[f64; 4], [_, t2, _, h2]: &[f64; 4]) -> bool {
    t1 < &(t2 + h2) && t2 < &(t1 + h1)
}

/// The image without empty rows (keeping `pad`), and how many rows were
/// dropped above.
fn trim_rows(rgba: Vec<u8>, width: usize, pad: usize) -> (Image, usize) {
    let row_len = width * 4;
    let rows = rgba.len() / row_len.max(1);
    let inked = |row: usize| {
        rgba[row * row_len..(row + 1) * row_len]
            .chunks(4)
            .any(|p| p[3] > 0)
    };
    let Some(first) = (0..rows).find(|&r| inked(r)) else {
        let empty = Image {
            width: width as u32,
            height: 0,
            rgba: Vec::new(),
            pieces: Vec::new(),
        };
        return (empty, 0);
    };
    let last = (0..rows).rev().find(|&r| inked(r)).unwrap_or(first);
    let top = first.saturating_sub(pad);
    let bottom = (last + 1 + pad).min(rows);
    let image = Image {
        width: width as u32,
        height: (bottom - top) as u32,
        rgba: rgba[top * row_len..bottom * row_len].to_vec(),
        pieces: Vec::new(),
    };
    (image, top)
}

/// Emoji font files shipped with the OS.
const EMOJI_FONTS: &[&str] = &[
    "/System/Library/Fonts/Apple Color Emoji.ttc",
    "C:\\Windows\\Fonts\\seguiemj.ttf",
    "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
    "/usr/share/fonts/noto/NotoColorEmoji.ttf",
    "/usr/share/fonts/google-noto-emoji/NotoColorEmoji.ttf",
];

/// The system emoji font, memory-mapped (Apple's is ~180 MB).
fn system_emoji_font() -> Option<Bytes> {
    EMOJI_FONTS.iter().find_map(|path| {
        let file = std::fs::File::open(path).ok()?;
        // Safety: system font files are not modified while we run.
        let map = unsafe { memmap2::Mmap::map(&file) }.ok()?;
        Some(Bytes::new(map))
    })
}

fn file_id(path: &str) -> FileId {
    RootedPath::new(
        VirtualRoot::Project,
        VirtualPath::new(path).expect("valid path"),
    )
    .intern()
}

struct MessageWorld<'a> {
    renderer: &'a Renderer,
    main: Source,
}

impl World for MessageWorld<'_> {
    fn library(&self) -> &LazyHash<Library> {
        &self.renderer.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.renderer.book
    }

    fn main(&self) -> FileId {
        self.main.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            return Ok(self.main.clone());
        }
        self.renderer
            .files
            .get(&id)
            .cloned()
            .ok_or_else(|| not_found(id))
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        Err(not_found(id))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.renderer.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}

fn not_found(id: FileId) -> FileError {
    FileError::NotFound(id.get().vpath().get_without_slash().into())
}

/// The whole message as a Typst program. All user text enters as string
/// literals, so it can never be interpreted as Typst markup or code.
/// `leaves` gets, for each text leaf, whether it is selected only whole.
fn typst_source(
    document: &Document,
    style: &RenderStyle,
    leaves: &mut Vec<bool>,
) -> Result<String, RenderError> {
    let mut body = String::new();
    blocks(&document.blocks, &mut body, leaves)?;
    let mut out = String::new();
    let _ = write!(
        out,
        r#"#import "/mitex/mod.typ": mitex-scope
// Typst does not grow lines for tall inline math, so it can stick out of
// the page; the vertical margin keeps it, and empty rows are trimmed after.
#set page(width: auto, height: auto, margin: (x: 0pt, y: {overflow}em), fill: none)
#set text(font: ({font}, {math}), size: {size}pt, fill: {text})
#set par(spacing: 1em, leading: 0.75em)
#show raw: set text(font: {mono}, size: 0.9em)
#show link: set text(fill: {link})
#let muted = {muted}
#let body = {body}
#context {{
  let width = measure(body).width
  block(width: calc.min(width, {max_width}pt), body)
}}
"#,
        font = string(&style.font_family),
        math = string(MATH_FONT),
        mono = string(MONO_FONT),
        size = style.font_size,
        text = color(style.text_color),
        link = color(style.link_color),
        muted = color(style.muted_color),
        max_width = style.max_width,
        overflow = OVERFLOW_EM,
    );
    Ok(out)
}

/// Puts invisible markers around text leaf number `leaves.len()`, so its
/// glyphs can be found in the layout; `atomic` if only selected whole.
fn marked(
    out: &mut String,
    leaves: &mut Vec<bool>,
    atomic: bool,
    content: impl FnOnce(&mut String) -> Result<(), RenderError>,
) -> Result<(), RenderError> {
    let leaf = leaves.len();
    leaves.push(atomic);
    let _ = write!(out, "metadata({});", 2 * leaf);
    content(out)?;
    let _ = write!(out, ";metadata({})", 2 * leaf + 1);
    Ok(())
}

fn blocks(list: &[Block], out: &mut String, leaves: &mut Vec<bool>) -> Result<(), RenderError> {
    out.push('{');
    for (i, block) in list.iter().enumerate() {
        if i > 0 {
            out.push_str("parbreak();");
        }
        match block {
            Block::Paragraph(content) => {
                out.push_str("par(");
                inlines(content, out, leaves)?;
                out.push(')');
            }
            Block::Heading { content, .. } => {
                out.push_str("text(weight: \"bold\", size: 1.15em, ");
                inlines(content, out, leaves)?;
                out.push(')');
            }
            Block::Quote(inner) => {
                out.push_str("block(inset: (left: 0.6em), stroke: (left: 2pt + muted), ");
                blocks(inner, out, leaves)?;
                out.push(')');
            }
            Block::List { start, items } => {
                match start {
                    Some(start) => {
                        let _ = write!(out, "enum(start: {start}, ");
                    }
                    None => out.push_str("list("),
                }
                for item in items {
                    blocks(item, out, leaves)?;
                    out.push_str(", ");
                }
                out.push(')');
            }
            Block::CodeBlock { language, code } => marked(out, leaves, true, |out| {
                out.push_str("block(fill: muted.transparentize(70%), inset: 0.4em, radius: 3pt, raw(block: true, ");
                if let Some(language) = language {
                    let _ = write!(out, "lang: {}, ", string(language));
                }
                out.push_str(&string(code.trim_end_matches('\n')));
                out.push_str("))");
                Ok(())
            })?,
            Block::Math(tex) => marked(out, leaves, true, |out| math(tex, true, out))?,
            Block::Rule => out.push_str("line(length: 100%, stroke: 0.5pt + muted)"),
        }
        out.push(';');
    }
    out.push('}');
    Ok(())
}

/// Inline content as one Typst expression.
fn inlines(
    content: &[Inline],
    out: &mut String,
    leaves: &mut Vec<bool>,
) -> Result<(), RenderError> {
    out.push('{');
    for inline in content {
        match inline {
            Inline::Text(text) => marked(out, leaves, false, |out| {
                out.push_str(&string(text));
                Ok(())
            })?,
            Inline::Styled { style, content } => {
                out.push_str(match style {
                    Style::Bold => "strong(",
                    Style::Italic => "emph(",
                    Style::Strikethrough => "strike(",
                    Style::Underline => "underline(",
                    Style::Superscript => "super(",
                    Style::Subscript => "sub(",
                    Style::Spoiler => "box(fill: muted, hide(",
                });
                inlines(content, out, leaves)?;
                out.push_str(if *style == Style::Spoiler { "))" } else { ")" });
            }
            Inline::Code(code) => marked(out, leaves, true, |out| {
                let _ = write!(out, "raw({})", string(code));
                Ok(())
            })?,
            Inline::Link { url, content } => {
                let _ = write!(out, "link({}, ", string(url));
                inlines(content, out, leaves)?;
                out.push(')');
            }
            Inline::Math(tex) => marked(out, leaves, true, |out| math(tex, false, out))?,
            Inline::LineBreak => out.push_str("linebreak()"),
        }
        out.push(';');
    }
    out.push('}');
    Ok(())
}

/// mitex 0.2.4 emits some Typst symbol names that Typst 0.14/0.15 renamed;
/// LaTeX command -> current Typst name.
const RENAMED_SYMBOLS: &[(&str, &str)] = &[
    ("cap", "inter"),
    ("Cap", "inter.double"),
    ("doublecap", "inter.double"),
    ("sqcap", "inter.sq"),
    ("bigcap", "inter.big"),
    ("partial", "partial"),
    ("odot", "dot.o"),
    ("bigodot", "dot.o.big"),
    ("bigdot", "dot.o.big"),
    ("oplus", "plus.o"),
    ("bigoplus", "plus.o.big"),
    ("bigplus", "plus.o.big"),
    ("otimes", "times.o"),
    ("bigotimes", "times.o.big"),
    ("bigtimes", "times.o.big"),
    ("ominus", "minus.o"),
    ("circledast", "ast.op.o"),
    ("circledcirc", "compose.o"),
    ("circleddash", "dash.o"),
    ("dashleftarrow", "arrow.l.dashed"),
    ("dashrightarrow", "arrow.r.dashed"),
    ("hbar", "planck"),
    ("hslash", "planck"),
    ("langle", "chevron.l"),
    ("lang", "chevron.l"),
    ("rangle", "chevron.r"),
    ("rang", "chevron.r"),
    ("llbracket", "bracket.l.stroked"),
    ("rrbracket", "bracket.r.stroked"),
    ("mho", "Omega.inv"),
    ("argmax", "op(\"arg max\", limits: #true)"),
    ("argmin", "op(\"arg min\", limits: #true)"),
    // Layout length for figures; meaningless in a message.
    ("textwidth", "zws"),
];

/// mitex's default command table with [`RENAMED_SYMBOLS`] applied.
static LATEX_SPEC: LazyLock<CommandSpec> = LazyLock::new(|| {
    let mut commands: fxhash::FxHashMap<String, CommandSpecItem> = mitex_spec_gen::DEFAULT_SPEC
        .items()
        .map(|(name, item)| (name.to_owned(), item.clone()))
        .collect();
    for (name, alias) in RENAMED_SYMBOLS {
        if let Some(CommandSpecItem::Cmd(shape)) = commands.get_mut(*name) {
            shape.alias = Some((*alias).to_owned());
        }
    }
    CommandSpec::new(commands)
});

fn math(tex: &str, block: bool, out: &mut String) -> Result<(), RenderError> {
    let converted = mitex::convert_math(tex, Some(LATEX_SPEC.clone())).map_err(|message| {
        RenderError::Latex {
            tex: tex.to_owned(),
            message,
        }
    })?;
    let _ = write!(
        out,
        "math.equation(block: {block}, eval({}, mode: \"math\", scope: mitex-scope))",
        string(&converted)
    );
    Ok(())
}

/// A Typst string literal.
fn string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn color([r, g, b, a]: [u8; 4]) -> String {
    format!("rgb({r}, {g}, {b}, {a})")
}
