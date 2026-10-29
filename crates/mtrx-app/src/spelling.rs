//! Spell checking of what one types: the system's checker on macOS
//! (`NSSpellChecker`) and Windows (`ISpellChecker`), Hunspell dictionaries
//! on Linux (`spellbook`). Each checks in all the languages the computer
//! has; code, math, links, addresses and emoji shortcodes are left out
//! ([`skip`]).

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    ops::Range,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

use iced::widget::TextInput;

use crate::style;

#[cfg(any(test, windows, all(unix, not(target_os = "macos"))))]
mod languages;
#[cfg(all(unix, not(target_os = "macos")))]
#[path = "spelling/linux.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "spelling/macos.rs"]
mod platform;
#[cfg(windows)]
#[path = "spelling/windows.rs"]
mod platform;
pub mod skip;

/// The most suggestions offered for a word.
const SUGGESTIONS: usize = 5;
/// The most texts whose misspellings are remembered.
const CACHED: usize = 256;

thread_local! {
    // The system's checkers are not to be shared between threads (COM on
    // Windows); iced draws and updates on one.
    static CHECKER: platform::Checker = platform::Checker::new();
    static IGNORED: RefCell<HashSet<String>> = RefCell::default();
    static CACHE: RefCell<HashMap<String, Vec<Range<usize>>>> = RefCell::default();
}

/// Whether to mark misspelled words (Settings' "Check spelling").
static ENABLED: AtomicBool = AtomicBool::new(true);
/// Counts the changes to what is marked (spell checking turned on, a word
/// learned or ignored), for the fields to mark their text again.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Turn the marking of misspelled words on or off.
pub fn set_enabled(on: bool) {
    if ENABLED.swap(on, Ordering::Relaxed) != on {
        changed();
    }
}

/// The marks of misspelled words to show: none while spell checking is off,
/// else a number that changes as words are learned or ignored, for a
/// field's highlighter to mark its text again.
pub fn marking() -> Option<u64> {
    ENABLED
        .load(Ordering::Relaxed)
        .then(|| GENERATION.load(Ordering::Relaxed))
}

/// `input`, showing `value`, with its misspelled words marked.
pub fn checked<'a, Message: Clone + 'a>(
    input: TextInput<'a, Message>,
    value: &str,
) -> TextInput<'a, Message> {
    let words = if marking().is_some() {
        misspelled(value)
    } else {
        Vec::new()
    };
    input.underlines(words, style::misspelled)
}

/// The misspelled words of `text`, a line, leaving out what [`skip`] skips
/// and the words ignored.
pub fn misspelled(text: &str) -> Vec<Range<usize>> {
    if let Some(cached) = CACHE.with_borrow(|cache| cache.get(text).cloned()) {
        return cached;
    }
    let skipped = skip::skipped(text);
    let found: Vec<_> = CHECKER.with(|checker| checker.misspelled(text));
    let words = IGNORED.with_borrow(|ignored| {
        found
            .into_iter()
            .filter(|word| !skipped.iter().any(|skip| overlaps(skip, word)))
            .filter(|word| !ignored.contains(&text[word.clone()]))
            .collect::<Vec<_>>()
    });
    CACHE.with_borrow_mut(|cache| {
        if cache.len() >= CACHED {
            cache.clear();
        }
        cache.insert(text.to_owned(), words.clone());
    });
    words
}

/// The misspelled word of `text` (a line) at byte `at`, if any.
pub fn word_at(text: &str, at: usize) -> Option<Range<usize>> {
    misspelled(text)
        .into_iter()
        .find(|word| word.start <= at && at <= word.end)
}

/// Suggestions for the misspelled `word` of `text`, best first.
pub fn suggestions(text: &str, word: Range<usize>) -> Vec<String> {
    let mut suggestions = CHECKER.with(|checker| checker.suggestions(text, word));
    suggestions.truncate(SUGGESTIONS);
    suggestions
}

/// Learn `word`, for good: on macOS and Windows in the system's dictionary,
/// which other apps share.
pub fn learn(word: &str) {
    CHECKER.with(|checker| checker.learn(word));
    changed();
}

/// Ignore `word` until the app quits.
pub fn ignore(word: &str) {
    IGNORED.with_borrow_mut(|ignored| ignored.insert(word.to_owned()));
    changed();
}

/// The languages checked, by name, for Settings.
pub fn languages() -> Vec<String> {
    CHECKER.with(platform::Checker::languages)
}

fn changed() {
    CACHE.with_borrow_mut(HashMap::clear);
    GENERATION.fetch_add(1, Ordering::Relaxed);
}

fn overlaps(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

/// The English name of the language with code `code` (`en`, `de-AT`,
/// `es_ES`): its name for a language `whatlang` knows, else the code.
pub fn language_name(code: &str) -> String {
    let primary = code.split(['-', '_']).next().unwrap_or(code);
    whatlang_of(primary).map_or_else(|| code.to_owned(), |lang| lang.eng_name().to_owned())
}

/// The `whatlang` language of an ISO 639-1 code (`en`), if it knows it.
pub fn whatlang_of(code: &str) -> Option<whatlang::Lang> {
    use whatlang::Lang;
    let lang = match code.to_ascii_lowercase().as_str() {
        "af" => Lang::Afr,
        "ak" => Lang::Aka,
        "am" => Lang::Amh,
        "ar" => Lang::Ara,
        "az" => Lang::Aze,
        "be" => Lang::Bel,
        "bg" => Lang::Bul,
        "bn" => Lang::Ben,
        "ca" => Lang::Cat,
        "cs" => Lang::Ces,
        "cy" => Lang::Cym,
        "da" => Lang::Dan,
        "de" => Lang::Deu,
        "el" => Lang::Ell,
        "en" => Lang::Eng,
        "eo" => Lang::Epo,
        "es" => Lang::Spa,
        "et" => Lang::Est,
        "fa" => Lang::Pes,
        "fi" => Lang::Fin,
        "fr" => Lang::Fra,
        "gu" => Lang::Guj,
        "he" => Lang::Heb,
        "hi" => Lang::Hin,
        "hr" => Lang::Hrv,
        "hu" => Lang::Hun,
        "hy" => Lang::Hye,
        "id" => Lang::Ind,
        "it" => Lang::Ita,
        "ja" => Lang::Jpn,
        "jv" => Lang::Jav,
        "ka" => Lang::Kat,
        "km" => Lang::Khm,
        "kn" => Lang::Kan,
        "ko" => Lang::Kor,
        "la" => Lang::Lat,
        "lt" => Lang::Lit,
        "lv" => Lang::Lav,
        "mk" => Lang::Mkd,
        "ml" => Lang::Mal,
        "mr" => Lang::Mar,
        "my" => Lang::Mya,
        "nb" | "no" => Lang::Nob,
        "ne" => Lang::Nep,
        "nl" => Lang::Nld,
        "or" => Lang::Ori,
        "pa" => Lang::Pan,
        "pl" => Lang::Pol,
        "pt" => Lang::Por,
        "ro" => Lang::Ron,
        "ru" => Lang::Rus,
        "si" => Lang::Sin,
        "sk" => Lang::Slk,
        "sl" => Lang::Slv,
        "sn" => Lang::Sna,
        "sr" => Lang::Srp,
        "sv" => Lang::Swe,
        "ta" => Lang::Tam,
        "te" => Lang::Tel,
        "th" => Lang::Tha,
        "tk" => Lang::Tuk,
        "tl" => Lang::Tgl,
        "tr" => Lang::Tur,
        "uk" => Lang::Ukr,
        "ur" => Lang::Urd,
        "uz" => Lang::Uzb,
        "vi" => Lang::Vie,
        "yi" => Lang::Yid,
        "zh" => Lang::Cmn,
        "zu" => Lang::Zul,
        _ => return None,
    };
    Some(lang)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Held by the tests of the system's checker, which is not to be used
    /// from two threads at once (in the app, iced's thread uses it).
    pub static SYSTEM: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn languages_are_named_by_their_code() {
        assert_eq!(language_name("en"), "English");
        assert_eq!(language_name("de-AT"), "German");
        assert_eq!(language_name("es_ES"), "Spanish");
        assert_eq!(language_name("xx"), "xx");
    }

    #[test]
    fn ignored_words_are_not_marked() {
        let _system = SYSTEM.lock();
        let text = "a wrod here";
        // Whatever the checker (none may be installed), an ignored word is
        // not among them.
        ignore("wrod");
        assert!(
            misspelled(text)
                .iter()
                .all(|word| &text[word.clone()] != "wrod")
        );
    }
}
