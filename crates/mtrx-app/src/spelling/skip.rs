//! What is not prose and so not checked: code, math, links, Matrix
//! addresses, e-mail addresses and emoji shortcodes.

use std::ops::Range;

/// Whether `line` opens or closes a block of code (```` ``` ````, `~~~`) or
/// of math (`$$` alone), whose lines are not checked.
pub fn is_fence(line: &str) -> bool {
    let line = line.trim();
    line.starts_with("```") || line.starts_with("~~~") || line == "$$"
}

/// The ranges of `line` not to check, in order.
pub fn skipped(line: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let bytes = line.as_bytes();
    let mut at = 0;
    while at < line.len() {
        let rest = &line[at..];
        let closed = |open: &str, close: &str| {
            rest.strip_prefix(open)
                .and_then(|inner| inner.find(close))
                .map(|end| at..at + open.len() + end + close.len())
        };
        let span = match bytes[at] {
            b'`' => {
                let ticks = rest.len() - rest.trim_start_matches('`').len();
                let fence = &rest[..ticks];
                Some(closed(fence, fence).unwrap_or(at..line.len()))
            }
            b'$' if rest.starts_with("$$") => closed("$$", "$$"),
            b'$' => closed("$", "$").filter(|span| {
                // `$5 and $6` is money, not math: math hugs its dollars.
                let inner = &line[span.start + 1..span.end - 1];
                !inner.starts_with(' ') && !inner.ends_with(' ')
            }),
            b'\\' if rest.starts_with("\\(") => closed("\\(", "\\)"),
            b'\\' if rest.starts_with("\\[") => closed("\\[", "\\]"),
            b']' if rest.starts_with("](") => closed("](", ")"),
            b'<' => closed("<", ">").filter(|span| !line[span.clone()].contains(' ')),
            b':' => closed(":", ":").filter(|span| {
                let name = &line[span.start + 1..span.end - 1];
                !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '+'))
            }),
            _ => None,
        };
        match span {
            Some(span) => {
                at = span.end;
                out.push(span);
            }
            None => at += rest.chars().next().map_or(1, char::len_utf8),
        }
    }
    out.extend(tokens(line).filter(|token| is_address(&line[token.clone()])));
    merged(out)
}

/// `spans` in order, those that overlap made one.
fn merged(mut spans: Vec<Range<usize>>) -> Vec<Range<usize>> {
    spans.sort_by_key(|span| span.start);
    let mut out: Vec<Range<usize>> = Vec::with_capacity(spans.len());
    for span in spans {
        match out.last_mut() {
            Some(last) if span.start < last.end => last.end = last.end.max(span.end),
            _ => out.push(span),
        }
    }
    out
}

/// The words of `line` as an address would run: between whitespace and
/// brackets, without the punctuation that ends a sentence around them.
fn tokens(line: &str) -> impl Iterator<Item = Range<usize>> + '_ {
    line.split(|c: char| c.is_whitespace() || matches!(c, '(' | '[' | '<' | '>'))
        .map(|token| token.trim_end_matches([')', ']', ',', '.', '!', '?', ';']))
        .filter(|token| !token.is_empty())
        .map(move |token| {
            let start = token.as_ptr() as usize - line.as_ptr() as usize;
            start..start + token.len()
        })
}

/// Whether `token` is a link, a Matrix address or an e-mail address.
fn is_address(token: &str) -> bool {
    token.contains("://")
        || token.starts_with("www.")
        || token.starts_with("matrix:")
        // @user:server, #room:server, !id:server; name@example.org.
        || (token.starts_with(['@', '#', '!']) && token.contains(':'))
        || token.char_indices().any(|(i, c)| c == '@' && i > 0 && token[i..].contains('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skipped_texts(line: &str) -> Vec<&str> {
        skipped(line).into_iter().map(|span| &line[span]).collect()
    }

    #[test]
    fn code_and_math_are_skipped() {
        assert_eq!(
            skipped_texts("run `cargo tset` and ``a ` b`` with $x^2$ or $$\\alph$$"),
            ["`cargo tset`", "``a ` b``", "$x^2$", "$$\\alph$$"]
        );
        assert_eq!(
            skipped_texts("\\(\\alph\\) and \\[y\\]"),
            ["\\(\\alph\\)", "\\[y\\]"]
        );
        // An unclosed code span runs to the end of the line.
        assert_eq!(skipped_texts("see `unfinishd"), ["`unfinishd"]);
    }

    #[test]
    fn money_is_not_math() {
        assert!(skipped("from $5 to $6 each").is_empty());
    }

    #[test]
    fn links_and_addresses_are_skipped() {
        assert_eq!(
            skipped_texts(
                "see [the paper](https://exmple.org/x) at https://ex.org, \
                 <https://a.b> or www.exmple.org"
            ),
            [
                "](https://exmple.org/x)",
                "https://ex.org",
                "<https://a.b>",
                "www.exmple.org"
            ]
        );
        assert_eq!(
            skipped_texts("ask @alx:matrix.org in #mtrx:matrix.org or mail alx@awi.de."),
            ["@alx:matrix.org", "#mtrx:matrix.org", "alx@awi.de"]
        );
    }

    #[test]
    fn emoji_shortcodes_are_skipped_but_not_colons_in_prose() {
        assert_eq!(
            skipped_texts("great :thumbsup: :+1:"),
            [":thumbsup:", ":+1:"]
        );
        assert!(skipped("Note: this is it: done").is_empty());
    }

    #[test]
    fn fences_open_and_close_blocks() {
        assert!(is_fence("```rust"));
        assert!(is_fence("  ~~~"));
        assert!(is_fence("$$"));
        assert!(!is_fence("$$x$$"));
        assert!(!is_fence("some ``` text"));
    }
}
