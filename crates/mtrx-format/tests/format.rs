use mtrx_format::{Block, Document, Inline, Style, markdown_to_matrix};

#[test]
fn plain_text_has_no_html() {
    let f = markdown_to_matrix("just text, 5 < 6");
    assert_eq!(f.body, "just text, 5 < 6");
    assert_eq!(f.html, None);
}

#[test]
fn markdown_and_line_breaks() {
    let f = markdown_to_matrix("**bold** and *it*\nnext line");
    assert_eq!(
        f.html.as_deref(),
        Some("<p><strong>bold</strong> and <em>it</em><br />\nnext line</p>")
    );
}

#[test]
fn math_uses_element_format() {
    let f = markdown_to_matrix(r"Energy $E=mc^2$ and $$\frac{a}{b} < 1$$");
    let html = f.html.unwrap();
    assert!(
        html.contains(r#"<span data-mx-maths="E=mc^2"><code>E=mc^2</code></span>"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<div data-mx-maths="\frac{a}{b} &lt; 1">"#),
        "{html}"
    );
    // The plain body keeps the LaTeX as typed.
    assert_eq!(f.body, r"Energy $E=mc^2$ and $$\frac{a}{b} < 1$$");
}

#[test]
fn dollar_amounts_are_not_math() {
    assert_eq!(markdown_to_matrix("it costs $5 and $10").html, None);
}

#[test]
fn typed_html_is_not_interpreted() {
    let html = markdown_to_matrix("<b>x</b> **y**").html.unwrap();
    assert!(html.contains("&lt;b&gt;x&lt;/b&gt;"), "{html}");
}

#[test]
fn parses_element_math() {
    let html = r#"Energy <span data-mx-maths="E=mc^2"><code>E=mc^2</code></span><div data-mx-maths="\sum_i x_i"><code>\sum_i x_i</code></div>"#;
    let doc = Document::from_html(html);
    assert_eq!(
        doc.blocks,
        vec![
            Block::Paragraph(vec![
                Inline::Text("Energy ".into()),
                Inline::Math("E=mc^2".into())
            ]),
            Block::Math(r"\sum_i x_i".into()),
        ]
    );
    assert!(doc.has_math());
    assert_eq!(doc.to_plain_text(), "Energy $E=mc^2$\n\n$$\\sum_i x_i$$");
}

#[test]
fn markdown_roundtrip_keeps_latex() {
    let text = r"Ratio $\alpha/\beta$ **bold**";
    let doc = Document::from_html(&markdown_to_matrix(text).html.unwrap());
    assert_eq!(doc.to_plain_text(), r"Ratio $\alpha/\beta$ bold");
}

#[test]
fn parses_structure_and_drops_reply_fallback() {
    let html = "<mx-reply><blockquote>old</blockquote></mx-reply><p>a <b>b</b> <a href=\"https://x.org\">link</a></p><ul><li>one</li><li>two</li></ul><pre><code class=\"language-rust\">fn x() {}\n</code></pre>";
    let doc = Document::from_html(html);
    assert_eq!(
        doc.blocks,
        vec![
            Block::Paragraph(vec![
                Inline::Text("a ".into()),
                Inline::Styled {
                    style: Style::Bold,
                    content: vec![Inline::Text("b".into())]
                },
                Inline::Text(" ".into()),
                Inline::Link {
                    url: "https://x.org".into(),
                    content: vec![Inline::Text("link".into())]
                },
            ]),
            Block::List {
                start: None,
                items: vec![
                    vec![Block::Paragraph(vec![Inline::Text("one".into())])],
                    vec![Block::Paragraph(vec![Inline::Text("two".into())])],
                ]
            },
            Block::CodeBlock {
                language: Some("rust".into()),
                code: "fn x() {}\n".into()
            },
        ]
    );
}

#[cfg(feature = "render")]
mod render {
    use mtrx_format::{Document, RenderError, RenderStyle, Renderer, markdown_to_matrix};

    fn style(max_width: f32) -> RenderStyle {
        RenderStyle {
            font_family: "Libertinus Serif".into(),
            font_size: 14.0,
            max_width,
            scale: 2.0,
            text_color: [20, 20, 20, 255],
            link_color: [30, 90, 200, 255],
            muted_color: [150, 150, 150, 255],
        }
    }

    fn doc(markdown: &str) -> Document {
        let f = markdown_to_matrix(markdown);
        f.html
            .map(|h| Document::from_html(&h))
            .unwrap_or_else(|| Document::from_plain(&f.body))
    }

    #[test]
    fn renders_common_latex() {
        let renderer = Renderer::new([]);
        for tex in [
            r"E=mc^2",
            r"\frac{\partial T}{\partial t} = \kappa \nabla^2 T",
            r"\sum_{i=1}^{n} x_i^2",
            r"\int_0^\infty e^{-x}\,dx",
            r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}",
            r"\begin{aligned} a &= b \\ c &= d \end{aligned}",
            r"\mathbb{R}^n \to \mathcal{F}",
            r"x \xrightarrow{f} y",
            r"\text{if } x > 0",
            r"\sqrt[3]{x} \leq \left( \frac{1}{2} \right)",
        ] {
            let image = renderer
                .render(&doc(&format!("$${tex}$$")), &style(400.0))
                .unwrap_or_else(|e| panic!("{tex}: {e}"));
            assert!(image.width > 0 && image.height > 0, "{tex}");
        }
    }

    /// Every zero-argument LaTeX symbol mitex knows must typeset with our Typst
    /// version (catches symbols renamed or removed by Typst upgrades).
    #[test]
    fn all_latex_symbols_render() {
        let spec = include_str!("../typst/mitex/latex/standard.typ");
        let renderer = Renderer::new([]);
        let mut failures = Vec::new();
        for line in spec.lines() {
            let Some((key, rest)) = line.trim().split_once(": ") else {
                continue;
            };
            let symbol =
                rest.starts_with("define-sym(") || rest.starts_with("of-sym(") || rest == "sym,";
            if !symbol || !key.chars().all(|c| c.is_ascii_alphabetic()) {
                continue;
            }
            let document = Document {
                blocks: vec![mtrx_format::Block::Math(format!("\\{key}"))],
            };
            if let Err(e) = renderer.render(&document, &style(400.0)) {
                failures.push(format!("\\{key}: {e}"));
            }
        }
        assert!(
            failures.is_empty(),
            "{} failures:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    #[test]
    fn wraps_at_max_width_and_shrinks_short_text() {
        let renderer = Renderer::new([]);
        let long = doc(&"word ".repeat(100));
        let image = renderer.render(&long, &style(200.0)).unwrap();
        assert!(image.width <= 400, "2x scale of 200 px: {}", image.width);
        let short = renderer.render(&doc("hi $x$"), &style(400.0)).unwrap();
        assert!(
            short.width < 200,
            "short message stays narrow: {}",
            short.width
        );
    }

    #[test]
    fn bad_latex_is_an_error_not_a_panic() {
        let renderer = Renderer::new([]);
        let result = renderer.render(&doc(r"$$\notacommand{x}$$"), &style(400.0));
        assert!(
            matches!(
                result,
                Err(RenderError::Latex { .. } | RenderError::Typst(_))
            ),
            "{result:?}"
        );
        // Unbalanced input is tolerated.
        assert!(renderer.render(&doc(r"$$\frac{a$$"), &style(400.0)).is_ok());
    }

    #[test]
    fn typst_syntax_in_text_is_literal() {
        let renderer = Renderer::new([]);
        // Would be code / markup if it were not escaped.
        let image = renderer.render(&doc(r#"#panic("x") \ " $x$ *a* [b]"#), &style(400.0));
        assert!(image.is_ok(), "{image:?}");
    }
}
