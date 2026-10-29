//! macOS's spell checker, `NSSpellChecker`: the languages ticked in System
//! Settings (Keyboard › Text Input › Spelling), found by itself per text in
//! its automatic mode, and the words learned in any app.

use std::ops::Range;

use objc2::rc::Retained;
use objc2_app_kit::NSSpellChecker;
use objc2_foundation::{NSInteger, NSNotFound, NSRange, NSString};

pub struct Checker {
    checker: Retained<NSSpellChecker>,
    /// The document our ignored words would be kept for; it keeps apart
    /// what other checks in this app learn of the text.
    tag: NSInteger,
}

impl Checker {
    pub fn new() -> Self {
        Self {
            checker: NSSpellChecker::sharedSpellChecker(),
            tag: NSSpellChecker::uniqueSpellDocumentTag(),
        }
    }

    pub fn misspelled(&self, text: &str) -> Vec<Range<usize>> {
        let string = NSString::from_str(text);
        let units = Utf16::new(text);
        let mut out = Vec::new();
        let mut at = 0;
        while at < units.len() {
            // SAFETY: the word count may be null.
            let found = unsafe {
                self.checker
                    .checkSpellingOfString_startingAt_language_wrap_inSpellDocumentWithTag_wordCount(
                        &string,
                        at as NSInteger,
                        None,
                        false,
                        self.tag,
                        std::ptr::null_mut(),
                    )
            };
            if found.location == NSNotFound as usize || found.length == 0 {
                break;
            }
            out.push(units.bytes(found.location)..units.bytes(found.location + found.length));
            at = found.location + found.length;
        }
        out
    }

    pub fn suggestions(&self, text: &str, word: Range<usize>) -> Vec<String> {
        let units = Utf16::new(text);
        let (start, end) = (units.units(word.start), units.units(word.end));
        self.checker
            .guessesForWordRange_inString_language_inSpellDocumentWithTag(
                NSRange::new(start, end - start),
                &NSString::from_str(text),
                None,
                self.tag,
            )
            .map(|guesses| guesses.iter().map(|guess| guess.to_string()).collect())
            .unwrap_or_default()
    }

    pub fn learn(&self, word: &str) {
        self.checker.learnWord(&NSString::from_str(word));
    }

    pub fn languages(&self) -> Vec<String> {
        let codes = if self.checker.automaticallyIdentifiesLanguages() {
            self.checker.userPreferredLanguages()
        } else {
            objc2_foundation::NSArray::from_retained_slice(&[self.checker.language()])
        };
        let mut names: Vec<String> = codes
            .iter()
            .map(|code| super::language_name(&code.to_string()))
            .collect();
        names.dedup();
        names
    }
}

/// Offsets in a text in UTF-16 units, as `NSString` counts, and in bytes.
struct Utf16 {
    /// The byte offset of each UTF-16 unit, and of the end.
    bytes: Vec<usize>,
}

impl Utf16 {
    fn new(text: &str) -> Self {
        let mut bytes = Vec::with_capacity(text.len() + 1);
        for (at, c) in text.char_indices() {
            bytes.extend(std::iter::repeat_n(at, c.len_utf16()));
        }
        bytes.push(text.len());
        Self { bytes }
    }

    /// The number of UTF-16 units.
    fn len(&self) -> usize {
        self.bytes.len() - 1
    }

    /// The byte offset of UTF-16 offset `units`.
    fn bytes(&self, units: usize) -> usize {
        self.bytes[units.min(self.len())]
    }

    /// The UTF-16 offset of byte offset `bytes`.
    fn units(&self, bytes: usize) -> usize {
        self.bytes.partition_point(|&at| at < bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_convert_between_bytes_and_utf16() {
        let text = "é😀a";
        let units = Utf16::new(text);
        assert_eq!(units.len(), 4);
        assert_eq!(units.bytes(1), 2);
        assert_eq!(units.bytes(3), 6);
        assert_eq!(units.units(2), 1);
        assert_eq!(units.units(6), 3);
        assert_eq!(units.units(7), 4);
    }

    #[test]
    fn misspelled_words_are_found_with_suggestions() {
        let _system = crate::spelling::tests::SYSTEM.lock();
        let checker = Checker::new();
        let text = "See you tommorow, I will chekc the draft.";
        let words: Vec<_> = checker
            .misspelled(text)
            .into_iter()
            .map(|word| &text[word])
            .collect();
        assert_eq!(words, ["tommorow", "chekc"]);
        let at = text.find("tommorow").unwrap();
        let suggestions = checker.suggestions(text, at..at + "tommorow".len());
        assert!(
            suggestions.iter().any(|s| s == "tomorrow"),
            "{suggestions:?}"
        );
    }
}
