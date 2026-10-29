//! Checking in several languages, a dictionary each (Windows, Linux): a
//! line is checked in the language it is written in, as `whatlang` tells
//! among those with a dictionary; when it cannot tell (a short line), a
//! word is right in any of them.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

/// The words of one language.
pub trait Dictionary {
    fn check(&self, word: &str) -> bool;
    /// Suggestions for a misspelled `word`, best first.
    fn suggest(&self, word: &str) -> Vec<String>;
}

/// How sure `whatlang` is to be of a line's language to check it in that
/// language only. Its own `is_reliable` also wants longer lines than most
/// messages.
const CONFIDENT: f64 = 0.8;

pub struct Language<D> {
    /// Its code, such as `en-US` or `de_DE`.
    pub code: String,
    pub dictionary: D,
}

pub struct Languages<D> {
    languages: Vec<Language<D>>,
    /// Among the languages `whatlang` knows; none if fewer than two.
    detector: Option<whatlang::Detector>,
}

impl<D: Dictionary> Languages<D> {
    pub fn new(languages: Vec<Language<D>>) -> Self {
        let mut known: Vec<_> = languages.iter().filter_map(|l| lang_of(&l.code)).collect();
        known.sort_by_key(|lang| lang.code());
        known.dedup();
        let detector = (known.len() > 1).then(|| whatlang::Detector::with_allowlist(known));
        Self {
            languages,
            detector,
        }
    }

    pub fn misspelled(&self, text: &str) -> Vec<Range<usize>> {
        let checking = self.checking(text);
        words(text)
            .filter(|word| {
                !checking
                    .clone()
                    .any(|language| language.dictionary.check(&text[word.clone()]))
            })
            .collect()
    }

    pub fn suggestions(&self, text: &str, word: Range<usize>) -> Vec<String> {
        let word = &text[word];
        let mut out: Vec<String> = Vec::new();
        for language in self.checking(text) {
            for suggestion in language.dictionary.suggest(word) {
                if !out.contains(&suggestion) {
                    out.push(suggestion);
                }
            }
        }
        out
    }

    /// The first language, if any.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn first(&self) -> Option<&Language<D>> {
        self.languages.first()
    }

    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<_> = self
            .languages
            .iter()
            .map(|language| super::language_name(&language.code))
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// The languages to check `text` in: the one it is surely written in,
    /// else all.
    fn checking<'a>(&'a self, text: &str) -> impl Iterator<Item = &'a Language<D>> + Clone {
        let written = self
            .detector
            .as_ref()
            .and_then(|detector| detector.detect(text))
            .filter(|info| info.confidence() >= CONFIDENT)
            .map(|info| info.lang());
        self.languages
            .iter()
            .filter(move |language| written.is_none() || lang_of(&language.code) == written)
    }
}

/// The `whatlang` language of a code such as `en-US`.
fn lang_of(code: &str) -> Option<whatlang::Lang> {
    super::whatlang_of(code.split(['-', '_']).next()?)
}

/// The words of `text` to check: those with a letter and no digit.
fn words(text: &str) -> impl Iterator<Item = Range<usize>> + '_ {
    text.split_word_bound_indices()
        .filter(|(_, word)| {
            word.chars().any(char::is_alphabetic) && !word.chars().any(|c| c.is_ascii_digit())
        })
        .map(|(at, word)| at..at + word.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Words(&'static [&'static str]);

    impl Dictionary for Words {
        fn check(&self, word: &str) -> bool {
            self.0.contains(&word.to_lowercase().as_str())
        }

        fn suggest(&self, word: &str) -> Vec<String> {
            self.0
                .iter()
                .filter(|w| w.len() == word.len())
                .map(|w| w.to_string())
                .collect()
        }
    }

    fn languages() -> Languages<Words> {
        Languages::new(vec![
            Language {
                code: "en-US".into(),
                dictionary: Words(&[
                    "the", "weather", "is", "very", "nice", "today", "and", "we", "go", "out", "a",
                ]),
            },
            Language {
                code: "es_ES".into(),
                dictionary: Words(&[
                    "el", "tiempo", "es", "muy", "bueno", "hoy", "y", "vamos", "a", "salir", "con",
                    "amigos",
                ]),
            },
        ])
    }

    fn misspelled(text: &str) -> Vec<&str> {
        languages()
            .misspelled(text)
            .into_iter()
            .map(|word| &text[word])
            .collect()
    }

    #[test]
    fn a_line_is_checked_in_its_language() {
        assert_eq!(
            misspelled("El tiempo es muy bueno hoy y vamos a slair con amigos"),
            ["slair"]
        );
        // "con" is a Spanish word, but not in an English line.
        assert_eq!(
            misspelled("The weather is very nice today and we go out con the weathr"),
            ["con", "weathr"]
        );
    }

    #[test]
    fn a_short_line_is_right_in_any_language() {
        assert_eq!(misspelled("hoy weather wrod"), ["wrod"]);
    }

    #[test]
    fn suggestions_come_from_the_line_s_language() {
        let text = "El tiempo es muy bueno hoy y vamos a slair con amigos";
        let at = text.find("slair").unwrap();
        // The Spanish five-letter words, not the English ones.
        assert_eq!(
            languages().suggestions(text, at..at + 5),
            ["bueno", "vamos", "salir"]
        );
    }

    #[test]
    fn numbers_and_punctuation_are_not_words() {
        assert!(misspelled("2026, 3rd: — !").is_empty());
    }

    #[test]
    fn languages_are_named_once() {
        assert_eq!(languages().names(), ["English", "Spanish"]);
    }
}
