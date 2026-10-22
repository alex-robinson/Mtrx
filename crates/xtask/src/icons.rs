//! `cargo xtask icons`: the iOS and Android icons and the website's logo,
//! made from the logo and committed, as the Xcode and Gradle builds run no
//! Rust. A test checks that the committed text files are current.

use std::{fmt::Write as _, path::Path};

use anyhow::{Context, Result, bail};
use resvg::{
    tiny_skia,
    usvg::{self, tiny_skia_path::PathSegment},
};

use super::LOGO;

/// One image, opaque: iOS rounds the corners itself.
const IOS_ICON: &str = "apps/ios/Mtrx/Assets.xcassets/AppIcon.appiconset/AppIcon.png";
/// On the sign-in screen, 96 pt.
const IOS_LOGO: &str = "apps/ios/Mtrx/Assets.xcassets/Logo.imageset/Logo.png";
const IOS_LOGO_SIZE: u32 = 512;
const ANDROID_BACKGROUND: &str =
    "apps/android/app/src/main/res/drawable/ic_launcher_background.xml";
const ANDROID_FOREGROUND: &str =
    "apps/android/app/src/main/res/drawable/ic_launcher_foreground.xml";
const ANDROID_LOGO: &str = "apps/android/app/src/main/res/drawable/logo.xml";
const SITE_LOGO: &str = "site/images/logo.svg";

/// The logo's rounded square, which full-bleed icons replace by their own.
const BACKGROUND_ID: &str = "background";
/// An adaptive icon's layers are 108 dp; launchers show at least the
/// circle of 66 dp in the middle.
const ADAPTIVE_SIZE: f32 = 108.0;
const ADAPTIVE_SAFE_DIAMETER: f32 = 66.0;

enum Contents {
    Text(String),
    Png(Vec<u8>),
}

/// Write every file made from the logo.
pub fn icons(root: &Path) -> Result<()> {
    for (path, contents) in outputs(root)? {
        let bytes = match contents {
            Contents::Text(text) => text.into_bytes(),
            Contents::Png(png) => png,
        };
        std::fs::write(root.join(path), bytes).with_context(|| format!("writing {path}"))?;
        println!("{path}");
    }
    Ok(())
}

fn outputs(root: &Path) -> Result<Vec<(&'static str, Contents)>> {
    let svg =
        std::fs::read_to_string(root.join(LOGO)).with_context(|| format!("reading {LOGO}"))?;
    let logo = Logo::parse(&svg)?;
    Ok(vec![
        (IOS_ICON, Contents::Png(logo.ios_icon()?)),
        (IOS_LOGO, Contents::Png(logo.png(IOS_LOGO_SIZE)?)),
        (
            ANDROID_BACKGROUND,
            Contents::Text(logo.android_background()),
        ),
        (
            ANDROID_FOREGROUND,
            Contents::Text(logo.android_foreground()),
        ),
        (ANDROID_LOGO, Contents::Text(logo.android_logo())),
        (SITE_LOGO, Contents::Text(svg)),
    ])
}

struct Logo {
    tree: usvg::Tree,
    /// The square's colour.
    background: usvg::Color,
    /// What stands on the square, as Android vector paths.
    marks: Vec<VectorPath>,
    /// The vector path of the square itself.
    square: VectorPath,
}

impl Logo {
    fn parse(svg: &str) -> Result<Self> {
        let tree = usvg::Tree::from_str(svg, &usvg::Options::default())?;
        let mut paths = Vec::new();
        collect_paths(tree.root(), &mut paths);
        let (squares, marks): (Vec<_>, Vec<_>) =
            paths.into_iter().partition(|p| p.id() == BACKGROUND_ID);
        let [square] = squares.as_slice() else {
            bail!("{LOGO} needs one path with the id \"{BACKGROUND_ID}\"");
        };
        let background = match square.fill().map(usvg::Fill::paint) {
            Some(usvg::Paint::Color(colour)) => *colour,
            _ => bail!("the logo's background is not one colour"),
        };
        Ok(Self {
            background,
            square: VectorPath::new(square)?,
            marks: marks
                .into_iter()
                .map(VectorPath::new)
                .collect::<Result<_>>()?,
            tree,
        })
    }

    fn size(&self) -> f32 {
        self.tree.size().width()
    }

    fn render(&self, pixmap: &mut tiny_skia::Pixmap) {
        let scale = pixmap.width() as f32 / self.size();
        resvg::render(
            &self.tree,
            tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
    }

    fn png(&self, size: u32) -> Result<Vec<u8>> {
        let mut pixmap = tiny_skia::Pixmap::new(size, size).context("empty size")?;
        self.render(&mut pixmap);
        Ok(pixmap.encode_png()?)
    }

    /// 1024 px, the square's colour to the edges, without alpha (which the
    /// App Store refuses).
    fn ios_icon(&self) -> Result<Vec<u8>> {
        const SIZE: u32 = 1024;
        let mut pixmap = tiny_skia::Pixmap::new(SIZE, SIZE).context("empty size")?;
        let usvg::Color { red, green, blue } = self.background;
        pixmap.fill(tiny_skia::Color::from_rgba8(red, green, blue, 255));
        self.render(&mut pixmap);
        let rgb: Vec<u8> = pixmap
            .pixels()
            .iter()
            .flat_map(|p| [p.red(), p.green(), p.blue()])
            .collect();
        let mut png = Vec::new();
        let mut encoder = png::Encoder::new(&mut png, SIZE, SIZE);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header()?.write_image_data(&rgb)?;
        Ok(png)
    }

    /// The square's colour, full bleed: the launcher masks it.
    fn android_background(&self) -> String {
        let size = ADAPTIVE_SIZE;
        format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<!-- Made by `cargo xtask icons` from {LOGO}:
     the square's colour, full bleed (the launcher masks it). -->
<vector xmlns:android="http://schemas.android.com/apk/res/android"
    android:width="{size}dp"
    android:height="{size}dp"
    android:viewportWidth="{size}"
    android:viewportHeight="{size}">
    <path
        android:fillColor="{fill}"
        android:pathData="M0,0h{size}v{size}h-{size}z" />
</vector>
"#,
            fill = colour(self.background),
        )
    }

    /// What stands on the square, scaled about the middle into the safe zone.
    fn android_foreground(&self) -> String {
        let middle = self.size() / 2.0;
        let reach = self
            .marks
            .iter()
            .map(|m| m.reach(middle))
            .fold(0.0, f32::max);
        let safe_radius = self.size() * ADAPTIVE_SAFE_DIAMETER / ADAPTIVE_SIZE / 2.0;
        // Rounded down, so that it stays inside.
        let scale = (safe_radius / reach * 1000.0).floor() / 1000.0;
        let mut paths = String::new();
        for mark in &self.marks {
            mark.write(&mut paths, "        ");
        }
        format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<!-- Made by `cargo xtask icons` from {LOGO}:
     what stands on the square, scaled into the adaptive icon's safe zone
     (66 of 108 dp). -->
<vector xmlns:android="http://schemas.android.com/apk/res/android"
    android:width="{ADAPTIVE_SIZE}dp"
    android:height="{ADAPTIVE_SIZE}dp"
    android:viewportWidth="{size}"
    android:viewportHeight="{size}">
    <group
        android:pivotX="{middle}"
        android:pivotY="{middle}"
        android:scaleX="{scale}"
        android:scaleY="{scale}">
{paths}    </group>
</vector>
"#,
            size = self.size(),
        )
    }

    /// The whole logo, for the sign-in screen.
    fn android_logo(&self) -> String {
        let mut paths = String::new();
        for path in std::iter::once(&self.square).chain(&self.marks) {
            path.write(&mut paths, "    ");
        }
        format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<!-- Made by `cargo xtask icons` from {LOGO}:
     the logo, for the sign-in screen. -->
<vector xmlns:android="http://schemas.android.com/apk/res/android"
    android:width="96dp"
    android:height="96dp"
    android:viewportWidth="{size}"
    android:viewportHeight="{size}">
{paths}</vector>
"#,
            size = self.size(),
        )
    }
}

fn collect_paths<'a>(group: &'a usvg::Group, paths: &mut Vec<&'a usvg::Path>) {
    for node in group.children() {
        match node {
            usvg::Node::Path(path) => paths.push(path),
            usvg::Node::Group(group) => collect_paths(group, paths),
            _ => {}
        }
    }
}

/// A path as an Android vector drawable draws it.
struct VectorPath {
    data: usvg::tiny_skia_path::Path,
    fill: Option<usvg::Color>,
    stroke: Option<Stroke>,
}

struct Stroke {
    colour: usvg::Color,
    width: f32,
    cap: &'static str,
    join: &'static str,
}

impl VectorPath {
    fn new(path: &usvg::Path) -> Result<Self> {
        let id = path.id();
        if !path.abs_transform().is_identity() {
            bail!("path {id:?} is transformed");
        }
        let colour = |paint: &usvg::Paint, opacity: usvg::Opacity| match paint {
            usvg::Paint::Color(c) if opacity.get() == 1.0 => Ok(*c),
            _ => bail!("path {id:?} is not painted in one opaque colour"),
        };
        let fill = path
            .fill()
            .map(|f| colour(f.paint(), f.opacity()))
            .transpose()?;
        let stroke = path
            .stroke()
            .map(|s| {
                Ok::<_, anyhow::Error>(Stroke {
                    colour: colour(s.paint(), s.opacity())?,
                    width: s.width().get(),
                    cap: match s.linecap() {
                        usvg::LineCap::Butt => "butt",
                        usvg::LineCap::Round => "round",
                        usvg::LineCap::Square => "square",
                    },
                    join: match s.linejoin() {
                        usvg::LineJoin::Round => "round",
                        usvg::LineJoin::Bevel => "bevel",
                        usvg::LineJoin::Miter | usvg::LineJoin::MiterClip => "miter",
                    },
                })
            })
            .transpose()?;
        Ok(Self {
            data: path.data().clone(),
            fill,
            stroke,
        })
    }

    /// How far it reaches from (`middle`, `middle`), at most: control
    /// points bound the curves, and strokes add half their width.
    fn reach(&self, middle: f32) -> f32 {
        let half_stroke = self.stroke.as_ref().map_or(0.0, |s| s.width / 2.0);
        self.data
            .points()
            .iter()
            .map(|p| (p.x - middle).hypot(p.y - middle) + half_stroke)
            .fold(0.0, f32::max)
    }

    fn write(&self, out: &mut String, indent: &str) {
        let _ = writeln!(out, "{indent}<path");
        if let Some(fill) = self.fill {
            let _ = writeln!(out, "{indent}    android:fillColor=\"{}\"", colour(fill));
        }
        if let Some(s) = &self.stroke {
            let _ = writeln!(
                out,
                "{indent}    android:strokeColor=\"{}\"",
                colour(s.colour)
            );
            let _ = writeln!(
                out,
                "{indent}    android:strokeWidth=\"{}\"",
                number(s.width)
            );
            let _ = writeln!(out, "{indent}    android:strokeLineCap=\"{}\"", s.cap);
            let _ = writeln!(out, "{indent}    android:strokeLineJoin=\"{}\"", s.join);
        }
        let _ = writeln!(
            out,
            "{indent}    android:pathData=\"{}\" />",
            self.path_data()
        );
    }

    fn path_data(&self) -> String {
        let point = |p: usvg::tiny_skia_path::Point| format!("{},{}", number(p.x), number(p.y));
        self.data
            .segments()
            .map(|segment| match segment {
                PathSegment::MoveTo(p) => format!("M{}", point(p)),
                PathSegment::LineTo(p) => format!("L{}", point(p)),
                PathSegment::QuadTo(p1, p) => format!("Q{} {}", point(p1), point(p)),
                PathSegment::CubicTo(p1, p2, p) => {
                    format!("C{} {} {}", point(p1), point(p2), point(p))
                }
                PathSegment::Close => "Z".into(),
            })
            .collect()
    }
}

/// `#AARRGGBB`, as Android writes colours.
fn colour(c: usvg::Color) -> String {
    format!("#FF{:02X}{:02X}{:02X}", c.red, c.green, c.blue)
}

/// To a tenth, without a trailing `.0` or the sign of a zero.
fn number(x: f32) -> String {
    let rounded = (x * 10.0).round() / 10.0 + 0.0;
    format!("{rounded}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Images may differ by a bit between platforms; text must not, but
    /// for line endings, which checkouts may change (in the logo too).
    #[test]
    fn committed_text_files_are_current() {
        let root = crate::workspace_root();
        let lines = |text: &str| text.replace("\r\n", "\n");
        for (path, contents) in outputs(&root).unwrap() {
            let Contents::Text(made) = contents else {
                continue;
            };
            let committed = std::fs::read_to_string(root.join(path)).unwrap();
            assert!(
                lines(&committed) == lines(&made),
                "{path} differs from what `cargo xtask icons` makes"
            );
        }
    }
}
