use mtrx_format::{
    Block, Document, Inline, Mention, MentionTarget, Style, find_mentions, markdown_to_matrix,
};

#[test]
fn plain_text_has_no_html() {
    let f = markdown_to_matrix("just text, 5 < 6", &[]);
    assert_eq!(f.body, "just text, 5 < 6");
    assert_eq!(f.html, None);
}

#[test]
fn markdown_and_line_breaks() {
    let f = markdown_to_matrix("**bold** and *it*\nnext line", &[]);
    assert_eq!(
        f.html.as_deref(),
        Some("<p><strong>bold</strong> and <em>it</em><br />\nnext line</p>")
    );
}

#[test]
fn math_uses_element_format() {
    let f = markdown_to_matrix(r"Energy $E=mc^2$ and $$\frac{a}{b} < 1$$", &[]);
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
    assert_eq!(markdown_to_matrix("it costs $5 and $10", &[]).html, None);
}

#[test]
fn typed_html_is_not_interpreted() {
    let html = markdown_to_matrix("<b>x</b> **y**", &[]).html.unwrap();
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
    assert_eq!(doc.formulas(), ["E=mc^2", r"\sum_i x_i"]);
    assert_eq!(doc.to_plain_text(), "Energy $E=mc^2$\n\n$$\\sum_i x_i$$");
}

#[test]
fn markdown_roundtrip_keeps_latex() {
    let text = r"Ratio $\alpha/\beta$ **bold**";
    let doc = Document::from_html(&markdown_to_matrix(text, &[]).html.unwrap());
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
        let f = markdown_to_matrix(markdown, &[]);
        f.html
            .map(|h| Document::from_html(&h))
            .unwrap_or_else(|| Document::from_plain(&f.body))
    }

    #[test]
    fn mentions_are_drawn_as_text_leaves() {
        let renderer = Renderer::new::<Vec<u8>>([]);
        let document = Document::from_html(
            r#"Ask <a href="https://matrix.to/#/@nils:server.org">Nils</a> about it"#,
        );
        let image = renderer.render(&document, &style(400.0)).unwrap();
        // The text, the mention, the text after it.
        let mention: Vec<_> = image.pieces.iter().filter(|p| p.leaf == 1).collect();
        let covered: usize = mention.iter().map(|p| p.range.clone().unwrap().len()).sum();
        assert_eq!(covered, "Nils".len());
        assert_eq!(image.pieces.last().map(|p| p.leaf), Some(2));
    }

    #[test]
    fn finds_where_the_text_is_drawn() {
        let renderer = Renderer::new::<Vec<u8>>([]);
        let text = "The melt rate";
        let document = doc(&format!("{text} $\\dot m$ is high.\n\n$$x^2$$"));
        let image = renderer.render(&document, &style(400.0)).unwrap();
        // Leaves: the text, the formula, the text after it, the displayed math.
        let leaves: Vec<_> = image.pieces.iter().map(|p| p.leaf).collect();
        assert!(leaves.is_sorted(), "reading order: {leaves:?}");
        assert_eq!(leaves.last(), Some(&3));
        // Each glyph of the text, left to right on one line.
        let first: Vec<_> = image.pieces.iter().filter(|p| p.leaf == 0).collect();
        let covered: usize = first.iter().map(|p| p.range.clone().unwrap().len()).sum();
        // The leaf is the text and the space before the formula.
        assert_eq!(covered, text.len() + 1);
        assert!(first.windows(2).all(|w| w[0].rect[0] < w[1].rect[0]));
        assert_eq!(first[0].range, Some(0..1));
        // The formula is one piece, right of the text, inside the image.
        let formula: Vec<_> = image.pieces.iter().filter(|p| p.leaf == 1).collect();
        assert_eq!(formula.len(), 1);
        assert_eq!(formula[0].range, None);
        let [x, y, w, h] = formula[0].rect;
        let last = first.last().unwrap().rect;
        assert!(x > last[0] && y >= 0.0 && y + h <= image.height as f32 && w > 0.0);
        // The displayed math is below.
        let display = image.pieces.iter().find(|p| p.leaf == 3).unwrap();
        assert!(display.rect[1] > y + h);
    }

    #[test]
    fn renders_common_latex() {
        let renderer = Renderer::new::<Vec<u8>>([]);
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
        let renderer = Renderer::new::<Vec<u8>>([]);
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
        let renderer = Renderer::new::<Vec<u8>>([]);
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
        let renderer = Renderer::new::<Vec<u8>>([]);
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
        let renderer = Renderer::new::<Vec<u8>>([]);
        // Would be code / markup if it were not escaped.
        let image = renderer.render(&doc(r#"#panic("x") \ " $x$ *a* [b]"#), &style(400.0));
        assert!(image.is_ok(), "{image:?}");
    }
}

#[test]
fn bare_urls_become_links() {
    let link = |url: &str| Inline::Link {
        url: url.into(),
        content: vec![Inline::Text(url.into())],
    };
    let doc = Document::from_plain("Join https://meet.google.com/abc-defg-hij now");
    assert_eq!(
        doc.blocks,
        vec![Block::Paragraph(vec![
            Inline::Text("Join ".into()),
            link("https://meet.google.com/abc-defg-hij"),
            Inline::Text(" now".into()),
        ])]
    );

    // Existing links, code and link text stay as they are.
    let doc = Document::from_html(
        "<b>see https://a.example/x</b> <a href=\"https://b.example\">https://c.example</a> \
         <code>https://d.example</code>",
    );
    assert_eq!(doc.links(), ["https://a.example/x", "https://b.example"]);
}

fn user(text: &str, id: &str, name: &str) -> MentionTarget {
    MentionTarget {
        text: text.into(),
        mention: Mention::User {
            href: format!("https://matrix.to/#/{id}"),
            name: name.into(),
        },
    }
}

/// Nils by name and ID, Nils Weitzel (a longer name), and the room.
fn targets() -> Vec<MentionTarget> {
    vec![
        user("Nils", "@nils:server.org", "Nils"),
        user("nils:server.org", "@nils:server.org", "Nils"),
        user("Nils Weitzel", "@weitzel:server.org", "Nils Weitzel"),
        MentionTarget {
            text: "room".into(),
            mention: Mention::Room,
        },
    ]
}

#[test]
fn mention_by_name_with_a_space() {
    let f = markdown_to_matrix("thanks @Nils Weitzel!", &targets());
    assert_eq!(
        f.html.as_deref(),
        Some(
            r#"<p>thanks <a href="https://matrix.to/#/@weitzel:server.org">Nils Weitzel</a>!</p>"#
        )
    );
    assert_eq!(f.body, "thanks @Nils Weitzel!");
    assert_eq!(f.mentions, [2]);
}

#[test]
fn mention_by_id() {
    let f = markdown_to_matrix("ask @nils:server.org.", &targets());
    assert_eq!(
        f.html.as_deref(),
        Some(r#"<p>ask <a href="https://matrix.to/#/@nils:server.org">Nils</a>.</p>"#)
    );
    assert_eq!(f.mentions, [1]);
}

#[test]
fn mention_of_the_room_stays_plain() {
    let f = markdown_to_matrix("@room lunch?", &targets());
    assert_eq!(f.html, None);
    assert_eq!(f.mentions, [3]);
    // Not a word that only starts with it.
    assert!(markdown_to_matrix("@rooms", &targets()).mentions.is_empty());
}

#[test]
fn unknown_names_are_text() {
    let f = markdown_to_matrix("hi @Anna and me@Nils", &targets());
    assert_eq!(f.html, None);
    assert!(f.mentions.is_empty());
}

#[test]
fn no_mentions_in_code_or_links() {
    let f = markdown_to_matrix(
        "`@Nils` [@Nils](https://x.org)\n\n```\n@Nils\n```",
        &targets(),
    );
    assert!(f.mentions.is_empty(), "{:?}", f.html);
    assert!(!f.html.unwrap().contains("matrix.to"));
}

#[test]
fn longest_name_wins() {
    let f = markdown_to_matrix("@Nils Weitzel and @Nils", &targets());
    assert_eq!(f.mentions, [2, 0]);
    let html = f.html.unwrap();
    assert!(html.contains(r#"<a href="https://matrix.to/#/@weitzel:server.org">Nils Weitzel</a> and <a href="https://matrix.to/#/@nils:server.org">Nils</a>"#), "{html}");
}

#[test]
fn mentions_stop_at_word_boundaries() {
    let f = markdown_to_matrix("@Nilsson and @Nils_x", &targets());
    assert!(f.mentions.is_empty());
    assert_eq!(f.html, None);
}

#[test]
fn mentions_at_the_start_and_end() {
    let f = markdown_to_matrix("@Nils see @Nils Weitzel", &targets());
    assert_eq!(f.mentions, [0, 2]);
    assert_eq!(
        f.html.as_deref(),
        Some(
            r#"<p><a href="https://matrix.to/#/@nils:server.org">Nils</a> see <a href="https://matrix.to/#/@weitzel:server.org">Nils Weitzel</a></p>"#
        )
    );
}

#[test]
fn finds_mentions_longest_first() {
    let targets = ["Nils", "Nils Weitzel", "room"];
    assert_eq!(
        find_mentions("@Nils Weitzel, @Nilsson @room @Nils", &targets),
        [(0..13, 1), (24..29, 2), (30..35, 0)]
    );
}

#[test]
fn matrix_links_to_users_are_mentions() {
    let doc = Document::from_html(
        r#"hi <a href="https://matrix.to/#/@nils:server.org">Nils <b>W</b></a>, <a href="matrix:u/anna:server.org">Anna</a> and <a href="https://matrix.to/#/!room:server.org">the room</a>"#,
    );
    let mention = |user_id: &str, name: &str| Inline::Mention {
        user_id: user_id.into(),
        name: name.into(),
    };
    assert_eq!(
        doc.blocks,
        vec![Block::Paragraph(vec![
            Inline::Text("hi ".into()),
            mention("@nils:server.org", "Nils W"),
            Inline::Text(", ".into()),
            mention("@anna:server.org", "Anna"),
            Inline::Text(" and ".into()),
            Inline::Link {
                url: "https://matrix.to/#/!room:server.org".into(),
                content: vec![Inline::Text("the room".into())],
            },
        ])]
    );
    assert_eq!(doc.links(), ["https://matrix.to/#/!room:server.org"]);
    assert_eq!(
        doc.to_plain_text(),
        "hi Nils W, Anna and the room (https://matrix.to/#/!room:server.org)"
    );

    // As sent.
    let sent = markdown_to_matrix("thanks @Nils Weitzel", &targets());
    let doc = Document::from_html(&sent.html.unwrap());
    assert_eq!(
        doc.blocks,
        vec![Block::Paragraph(vec![
            Inline::Text("thanks ".into()),
            mention("@weitzel:server.org", "Nils Weitzel"),
        ])]
    );
}

#[test]
fn copied_as_html_with_math_as_latex() {
    let doc = Document::from_html(
        r#"<p>Flux <b>q</b> <span data-mx-maths="q=-k\nabla T"><code>q</code></span>, see <a href="https://example.org/a">data</a> &amp; <a href="https://matrix.to/#/@ana:x.org">Ana</a></p><ol start="3"><li>one</li><li>two<br>lines</li></ol><pre><code class="language-rust">fn x() {}
</code></pre><div data-mx-maths="x^2"><code>x^2</code></div>"#,
    );
    assert_eq!(
        doc.to_html(),
        r#"<p>Flux <strong>q</strong> $q=-k\nabla T$, see <a href="https://example.org/a">data</a> &amp; <a href="https://matrix.to/#/@ana:x.org">Ana</a></p><ol start="3"><li>one</li><li>two<br>lines</li></ol><pre><code class="language-rust">fn x() {}
</code></pre><p>$$x^2$$</p>"#
    );
}

#[test]
fn clipped_to_a_selection() {
    // Leaves: "Hello ", "bold", " world", "one", "two", "three".
    let doc = Document::from_html(
        "<p>Hello <b>bold</b> world</p><hr><ul><li>one</li><li>two</li><li>three</li></ul>",
    );
    let part = doc.clipped(|leaf| match leaf {
        1 => Some(2..4),
        2 => Some(0..6),
        3 => Some(0..3),
        4 => Some(0..2),
        _ => None,
    });
    assert_eq!(
        part.to_html(),
        "<p><strong>ld</strong> world</p><ul><li>one</li><li>tw</li></ul>"
    );
    // All of it, but the rule.
    let mut all = doc.clone();
    all.blocks.remove(1);
    assert_eq!(doc.clipped(|_| Some(0..usize::MAX)), all);

    // Numbered from the first item kept.
    let list = Document::from_html("<ol><li>a</li><li>b</li><li>c</li></ol>");
    let part = list.clipped(|leaf| (leaf == 1).then_some(0..1));
    assert_eq!(part.to_html(), r#"<ol start="2"><li>b</li></ol>"#);

    // Part of a formula is text, as shown; all of it stays math.
    let math = Document::from_html(
        r#"Energy <span data-mx-maths="E=mc^2"><code>E=mc^2</code></span><br>and more"#,
    );
    let part = math.clipped(|leaf| (leaf == 1).then_some(0..4));
    assert_eq!(
        part.blocks,
        [Block::Paragraph(vec![Inline::Text("$E=m".into())])]
    );
    let part = math.clipped(|leaf| (leaf == 1).then_some(0..8));
    assert_eq!(part.to_html(), "<p>$E=mc^2$</p>", "without the line break");
}
