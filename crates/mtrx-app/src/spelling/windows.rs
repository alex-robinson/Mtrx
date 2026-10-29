//! Spell checking on Windows: the system's checker (`ISpellChecker`), one
//! for each language with a dictionary, which Windows installs with each
//! language added in Settings › Time & language. Words learned go into the
//! user's dictionary, which other apps share.

use std::ops::Range;

use tracing::warn;
use windows::{
    Win32::{
        Globalization::{ISpellChecker, ISpellCheckerFactory, SpellCheckerFactory},
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
            CoTaskMemFree, IEnumString,
        },
    },
    core::{HSTRING, PWSTR},
};

use super::languages::{Dictionary, Language, Languages};

pub struct Checker {
    languages: Languages<System>,
}

impl Checker {
    pub fn new() -> Self {
        let languages = installed().unwrap_or_else(|e| {
            warn!("no spell checker: {e}");
            Vec::new()
        });
        Self {
            languages: Languages::new(languages),
        }
    }

    pub fn misspelled(&self, text: &str) -> Vec<Range<usize>> {
        self.languages.misspelled(text)
    }

    pub fn suggestions(&self, text: &str, word: Range<usize>) -> Vec<String> {
        self.languages.suggestions(text, word)
    }

    pub fn learn(&self, word: &str) {
        // The user's dictionary is one for all languages; adding through one
        // checker adds it for all.
        if let Some(language) = self.languages.first()
            && let Err(e) = unsafe { language.dictionary.0.Add(&HSTRING::from(word)) }
        {
            warn!("could not learn the word: {e}");
        }
    }

    pub fn languages(&self) -> Vec<String> {
        self.languages.names()
    }
}

/// A checker for each language Windows has a dictionary of.
fn installed() -> windows::core::Result<Vec<Language<System>>> {
    // COM, in the apartment of the window's thread; already so is fine.
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let factory: ISpellCheckerFactory =
        unsafe { CoCreateInstance(&SpellCheckerFactory, None, CLSCTX_INPROC_SERVER)? };
    let codes = strings(&unsafe { factory.SupportedLanguages()? });
    Ok(codes
        .into_iter()
        .filter_map(|code| {
            let checker = unsafe { factory.CreateSpellChecker(&HSTRING::from(&code)) }
                .inspect_err(|e| warn!(code, "no spell checker: {e}"))
                .ok()?;
            Some(Language {
                code,
                dictionary: System(checker),
            })
        })
        .collect())
}

/// The strings of `strings`, freeing them.
fn strings(strings: &IEnumString) -> Vec<String> {
    let mut out = Vec::new();
    loop {
        let mut next = [PWSTR::null()];
        let mut fetched = 0;
        let result = unsafe { strings.Next(&mut next, Some(&mut fetched)) };
        if result.is_err() || fetched == 0 {
            return out;
        }
        // SAFETY: a string from COM, ours to free.
        unsafe {
            if let Ok(string) = next[0].to_string() {
                out.push(string);
            }
            CoTaskMemFree(Some(next[0].0 as *const _));
        }
    }
}

struct System(ISpellChecker);

impl Dictionary for System {
    fn check(&self, word: &str) -> bool {
        let Ok(errors) = (unsafe { self.0.Check(&HSTRING::from(word)) }) else {
            return true;
        };
        let mut error = None;
        // S_OK with an error; S_FALSE when there are none.
        let found = unsafe { errors.Next(&mut error) };
        !(found.is_ok() && error.is_some())
    }

    fn suggest(&self, word: &str) -> Vec<String> {
        unsafe { self.0.Suggest(&HSTRING::from(word)) }
            .map(|suggestions| strings(&suggestions))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn misspelled_words_are_found_with_suggestions() {
        let checker = Checker::new();
        // The runners have English.
        assert!(checker.languages().contains(&"English".to_owned()));
        let text = "This sentense is in English";
        let words: Vec<_> = checker
            .misspelled(text)
            .into_iter()
            .map(|word| &text[word])
            .collect();
        assert_eq!(words, ["sentense"]);
        let suggestions = checker.suggestions(text, 5..13);
        assert!(
            suggestions.iter().any(|s| s == "sentence"),
            "{suggestions:?}"
        );
    }
}
