//! Typeset a [`Document`] with Typst into an RGBA image. LaTeX math is
//! converted to Typst math with `mitex`.

use std::{collections::HashMap, fmt::Write, sync::LazyLock};

use mitex::{CommandSpec, CommandSpecItem};

use typst::{
    Library, LibraryExt, World,
    diag::{FileError, FileResult},
    foundations::{Bytes, Datetime, Duration},
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

/// Straight (not premultiplied) RGBA pixels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
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
        let source = Source::new(self.main_id, typst_source(document, style)?);
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
        Ok(trim_rows(rgba, width, (style.scale * 2.0) as usize))
    }
}

/// Drop fully transparent rows above and below the content, keeping `pad`.
fn trim_rows(rgba: Vec<u8>, width: usize, pad: usize) -> Image {
    let row_len = width * 4;
    let rows = rgba.len() / row_len.max(1);
    let inked = |row: usize| {
        rgba[row * row_len..(row + 1) * row_len]
            .chunks(4)
            .any(|p| p[3] > 0)
    };
    let Some(first) = (0..rows).find(|&r| inked(r)) else {
        return Image {
            width: width as u32,
            height: 0,
            rgba: Vec::new(),
        };
    };
    let last = (0..rows).rev().find(|&r| inked(r)).unwrap_or(first);
    let top = first.saturating_sub(pad);
    let bottom = (last + 1 + pad).min(rows);
    Image {
        width: width as u32,
        height: (bottom - top) as u32,
        rgba: rgba[top * row_len..bottom * row_len].to_vec(),
    }
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
fn typst_source(document: &Document, style: &RenderStyle) -> Result<String, RenderError> {
    let mut body = String::new();
    blocks(&document.blocks, &mut body)?;
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

fn blocks(list: &[Block], out: &mut String) -> Result<(), RenderError> {
    out.push('{');
    for (i, block) in list.iter().enumerate() {
        if i > 0 {
            out.push_str("parbreak();");
        }
        match block {
            Block::Paragraph(content) => {
                out.push_str("par(");
                inlines(content, out)?;
                out.push(')');
            }
            Block::Heading { content, .. } => {
                out.push_str("text(weight: \"bold\", size: 1.15em, ");
                inlines(content, out)?;
                out.push(')');
            }
            Block::Quote(inner) => {
                out.push_str("block(inset: (left: 0.6em), stroke: (left: 2pt + muted), ");
                blocks(inner, out)?;
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
                    blocks(item, out)?;
                    out.push_str(", ");
                }
                out.push(')');
            }
            Block::CodeBlock { language, code } => {
                out.push_str("block(fill: muted.transparentize(70%), inset: 0.4em, radius: 3pt, raw(block: true, ");
                if let Some(language) = language {
                    let _ = write!(out, "lang: {}, ", string(language));
                }
                out.push_str(&string(code.trim_end_matches('\n')));
                out.push_str("))");
            }
            Block::Math(tex) => math(tex, true, out)?,
            Block::Rule => out.push_str("line(length: 100%, stroke: 0.5pt + muted)"),
        }
        out.push(';');
    }
    out.push('}');
    Ok(())
}

/// Inline content as one Typst expression.
fn inlines(content: &[Inline], out: &mut String) -> Result<(), RenderError> {
    out.push('{');
    for inline in content {
        match inline {
            Inline::Text(text) => out.push_str(&string(text)),
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
                inlines(content, out)?;
                out.push_str(if *style == Style::Spoiler { "))" } else { ")" });
            }
            Inline::Code(code) => {
                let _ = write!(out, "raw({})", string(code));
            }
            Inline::Link { url, content } => {
                let _ = write!(out, "link({}, ", string(url));
                inlines(content, out)?;
                out.push(')');
            }
            Inline::Math(tex) => math(tex, false, out)?,
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
