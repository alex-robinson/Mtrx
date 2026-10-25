//! Mtrx patch (surrounding text): the text a text input client shows AppKit, made of the
//! application's [`ImeSurroundingText`] and the marked text, in AppKit's UTF-16 offsets; and a
//! range AppKit replaces in it, as the bytes to delete around the selection.

use crate::window::ImeSurroundingText;

/// The text as AppKit sees it: the surrounding text with the marked text, if any, in place of
/// the selection, as a text view shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Document<'a> {
    /// The text before the selection.
    before: &'a str,
    /// The marked text, or else the selection.
    middle: &'a str,
    /// The text after the selection.
    after: &'a str,
}

impl<'a> Document<'a> {
    pub fn new(surrounding: &'a ImeSurroundingText, marked: &'a str) -> Self {
        let text = surrounding.text();
        let low = surrounding.cursor().min(surrounding.anchor());
        let high = surrounding.cursor().max(surrounding.anchor());
        let middle = if marked.is_empty() { &text[low..high] } else { marked };

        Self { before: &text[..low], middle, after: &text[high..] }
    }

    /// Where the marked text or the selection starts.
    pub fn middle_start(&self) -> usize {
        utf16_len(self.before)
    }

    /// The length of the marked text or the selection.
    pub fn middle_len(&self) -> usize {
        utf16_len(self.middle)
    }

    pub fn len(&self) -> usize {
        self.middle_start() + self.middle_len() + utf16_len(self.after)
    }

    /// The text from `start`, `len` long, cut to the text and widened to whole characters, with
    /// the range it covers.
    pub fn substring(&self, start: usize, len: usize) -> (String, usize, usize) {
        let text = [self.before, self.middle, self.after].concat();
        let end = start.saturating_add(len).min(self.len());
        let start = start.min(end);

        let mut from = 0;
        let mut to = text.len();
        let mut at = 0;
        let (mut from_utf16, mut to_utf16) = (0, self.len());
        for (index, character) in text.char_indices() {
            let next = at + character.len_utf16();
            if next <= start {
                from = index + character.len_utf8();
                from_utf16 = next;
            }
            if at >= end {
                to = index;
                to_utf16 = at;
                break;
            }
            at = next;
        }

        (text[from..to].to_owned(), from_utf16, to_utf16 - from_utf16)
    }

    /// The bytes to delete before and after the selection to replace the range from `start`,
    /// `len` long: the marked text or the selection goes with the commit or preedit that
    /// follows. `None` if there is nothing more to delete, or the range does not reach the
    /// selection, or splits a character.
    pub fn deletion(&self, start: usize, len: usize) -> Option<(usize, usize)> {
        let end = start.checked_add(len)?;
        let middle_start = self.middle_start();
        let middle_end = middle_start + self.middle_len();
        if start > middle_end || end < middle_start || end > self.len() {
            return None;
        }

        let before = if start < middle_start {
            self.before.len() - utf16_to_byte(self.before, start)?
        } else {
            0
        };
        let after = if end > middle_end { utf16_to_byte(self.after, end - middle_end)? } else { 0 };

        (before, after).ne(&(0, 0)).then_some((before, after))
    }
}

pub(super) fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// The byte offset of the UTF-16 offset `utf16` in `text`; `None` inside a character or past
/// the end.
pub(super) fn utf16_to_byte(text: &str, utf16: usize) -> Option<usize> {
    let mut at = 0;
    for (index, character) in text.char_indices() {
        if at == utf16 {
            return Some(index);
        }
        at += character.len_utf16();
        if at > utf16 {
            return None;
        }
    }

    (at == utf16).then_some(text.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn surrounding(text: &str, cursor: usize, anchor: usize) -> ImeSurroundingText {
        ImeSurroundingText::new(text.to_owned(), cursor, anchor).unwrap()
    }

    #[test]
    fn utf16_offsets_to_bytes() {
        // "é" is one UTF-16 unit and two bytes, "😀" two units and four bytes.
        let text = "aé😀b";
        assert_eq!(utf16_len(text), 5);
        assert_eq!(utf16_to_byte(text, 0), Some(0));
        assert_eq!(utf16_to_byte(text, 2), Some(3));
        assert_eq!(utf16_to_byte(text, 3), None);
        assert_eq!(utf16_to_byte(text, 4), Some(7));
        assert_eq!(utf16_to_byte(text, 5), Some(8));
        assert_eq!(utf16_to_byte(text, 6), None);
    }

    #[test]
    fn the_selection_or_the_marked_text_in_the_middle() {
        // "Crè|me", the caret after "è".
        let text = surrounding("Crème", 4, 4);
        let document = Document::new(&text, "");
        assert_eq!((document.middle_start(), document.middle_len(), document.len()), (3, 0, 5));
        // "è" selected backwards, then replaced by the marked text "´".
        let text = surrounding("Crème", 2, 4);
        let document = Document::new(&text, "");
        assert_eq!((document.middle_start(), document.middle_len()), (2, 1));
        let document = Document::new(&text, "´e");
        assert_eq!((document.middle_start(), document.middle_len(), document.len()), (2, 2, 6));
        assert_eq!(document.substring(0, 6), ("Cr´eme".to_owned(), 0, 6));
    }

    #[test]
    fn substrings_are_cut_to_the_text_and_whole_characters() {
        let text = surrounding("a😀b", 0, 0);
        let document = Document::new(&text, "");
        assert_eq!(document.substring(0, 1), ("a".to_owned(), 0, 1));
        // Half of "😀" takes all of it.
        assert_eq!(document.substring(2, 2), ("😀b".to_owned(), 1, 3));
        assert_eq!(document.substring(1, 1), ("😀".to_owned(), 1, 2));
        assert_eq!(document.substring(3, 10), ("b".to_owned(), 3, 1));
        assert_eq!(document.substring(10, 1), (String::new(), 4, 0));
    }

    #[test]
    fn a_replacement_range_as_bytes_to_delete() {
        // Holding "è" after "Cr": AppKit replaces it, the range before the caret.
        let text = surrounding("Crè", 4, 4);
        let document = Document::new(&text, "");
        assert_eq!(document.deletion(2, 1), Some((2, 0)));
        // Across the caret.
        let text = surrounding("Crème", 4, 4);
        let document = Document::new(&text, "");
        assert_eq!(document.deletion(1, 3), Some((3, 1)));
        // The caret alone, or no range: nothing to delete.
        assert_eq!(document.deletion(3, 0), None);
        // Not reaching the caret, or past the end: not something to delete around it.
        assert_eq!(document.deletion(0, 1), None);
        assert_eq!(document.deletion(4, 1), None);
        assert_eq!(document.deletion(3, 3), None);
        // Around a selection, which the commit replaces: "è" selected.
        let text = surrounding("Crème", 4, 2);
        let document = Document::new(&text, "");
        assert_eq!(document.deletion(1, 3), Some((1, 1)));
        assert_eq!(document.deletion(2, 1), None);
        // Around the marked text, in its place.
        let document = Document::new(&text, "ab");
        assert_eq!(document.deletion(0, 5), Some((2, 1)));
        // Splitting "😀".
        let text = surrounding("😀a", 4, 4);
        let document = Document::new(&text, "");
        assert_eq!(document.deletion(1, 1), None);
        assert_eq!(document.deletion(0, 2), Some((4, 0)));
    }
}
