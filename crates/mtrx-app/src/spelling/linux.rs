//! Spell checking on Linux: the system's Hunspell dictionaries (packages
//! such as `hunspell-en-us` and `hunspell-es`), each read when first
//! needed, with `spellbook`. Words learned are kept in `words.txt` in the
//! app's data directory.

use std::{
    cell::{OnceCell, RefCell},
    collections::{BTreeMap, HashSet},
    fs,
    io::Write,
    ops::Range,
    path::{Path, PathBuf},
};

use tracing::warn;

use super::languages::{Dictionary, Language, Languages};

/// Where distributions put Hunspell dictionaries, after `$DICPATH`.
const DIRS: [&str; 4] = [
    "/usr/share/hunspell",
    "/usr/share/myspell/dicts",
    "/usr/share/myspell",
    "/usr/local/share/hunspell",
];

pub struct Checker {
    languages: Languages<Lazy>,
    learned: RefCell<HashSet<String>>,
}

impl Checker {
    pub fn new() -> Self {
        let mut dirs: Vec<PathBuf> = std::env::var_os("DICPATH")
            .map(|paths| std::env::split_paths(&paths).collect())
            .unwrap_or_default();
        dirs.extend(DIRS.iter().map(PathBuf::from));
        if let Some(home) = std::env::var_os("HOME") {
            dirs.push(Path::new(&home).join(".local/share/hunspell"));
        }
        let learned = learned_path()
            .and_then(|path| fs::read_to_string(path).ok())
            .map(|words| words.lines().map(str::to_owned).collect())
            .unwrap_or_default();
        Self {
            languages: Languages::new(installed(&dirs)),
            learned: RefCell::new(learned),
        }
    }

    pub fn misspelled(&self, text: &str) -> Vec<Range<usize>> {
        let learned = self.learned.borrow();
        self.languages
            .misspelled(text)
            .into_iter()
            .filter(|word| !learned.contains(&text[word.clone()]))
            .collect()
    }

    pub fn suggestions(&self, text: &str, word: Range<usize>) -> Vec<String> {
        self.languages.suggestions(text, word)
    }

    pub fn learn(&self, word: &str) {
        self.learned.borrow_mut().insert(word.to_owned());
        let Some(path) = learned_path() else { return };
        let appended = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut file| writeln!(file, "{word}"));
        if let Err(e) = appended {
            warn!(path = %path.display(), "could not keep the word learned: {e}");
        }
    }

    pub fn languages(&self) -> Vec<String> {
        self.languages.names()
    }
}

/// The file of the words learned.
fn learned_path() -> Option<PathBuf> {
    mtrx_core::Profile::default_data_root()
        .ok()
        .map(|root| root.join("words.txt"))
}

/// The dictionaries in `dirs`, one for each language and region (the
/// first found): those with both an `.aff` and a `.dic` file.
fn installed(dirs: &[PathBuf]) -> Vec<Language<Lazy>> {
    let mut found = BTreeMap::new();
    for dir in dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "dic")
                && path.with_extension("aff").exists()
                && let Some(code) = path.file_stem().and_then(|s| s.to_str())
                && super::whatlang_of(code.split(['-', '_']).next().unwrap_or(code)).is_some()
            {
                found.entry(code.to_owned()).or_insert(path);
            }
        }
    }
    found
        .into_iter()
        .map(|(code, dic)| Language {
            code,
            dictionary: Lazy {
                dic,
                loaded: OnceCell::new(),
            },
        })
        .collect()
}

/// A dictionary read when first needed; none if it cannot be read.
struct Lazy {
    dic: PathBuf,
    loaded: OnceCell<Option<spellbook::Dictionary>>,
}

impl Lazy {
    fn get(&self) -> Option<&spellbook::Dictionary> {
        self.loaded
            .get_or_init(|| {
                let read = |path: &Path| fs::read(path).map(|bytes| decoded(&bytes));
                let loaded = read(&self.dic.with_extension("aff")).and_then(|aff| {
                    let dic = read(&self.dic)?;
                    spellbook::Dictionary::new(&aff, &dic)
                        .map_err(|e| std::io::Error::other(e.to_string()))
                });
                loaded
                    .inspect_err(
                        |e| warn!(dic = %self.dic.display(), "could not read the dictionary: {e}"),
                    )
                    .ok()
            })
            .as_ref()
    }
}

impl Dictionary for Lazy {
    fn check(&self, word: &str) -> bool {
        // A dictionary that cannot be read takes every word.
        self.get().is_none_or(|dictionary| dictionary.check(word))
    }

    fn suggest(&self, word: &str) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(dictionary) = self.get() {
            dictionary.suggest(word, &mut out);
        }
        out
    }
}

/// The text of a dictionary file: UTF-8, or else ISO 8859-1, which older
/// dictionaries use.
fn decoded(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(_) => bytes.iter().map(|&b| char::from(b)).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionaries_are_found_once_per_code() {
        let dir = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        for (dir, name) in [
            (dir.path(), "en_US"),
            (dir.path(), "es_ES"),
            (other.path(), "en_US"),
            (dir.path(), "hyph_de_DE"),
        ] {
            fs::write(dir.join(format!("{name}.dic")), "1\nword\n").unwrap();
            if !name.starts_with("hyph") {
                fs::write(dir.join(format!("{name}.aff")), "SET UTF-8\n").unwrap();
            }
        }
        let found = installed(&[dir.path().to_owned(), other.path().to_owned()]);
        let codes: Vec<_> = found.iter().map(|l| l.code.as_str()).collect();
        assert_eq!(codes, ["en_US", "es_ES"]);
        assert!(found[0].dictionary.dic.starts_with(dir.path()));
        assert!(found[0].dictionary.check("word"));
    }

    #[test]
    fn latin_1_dictionaries_are_read() {
        assert_eq!(decoded(b"Gr\xfc\xdfe"), "Grüße");
        assert_eq!(decoded("Grüße".as_bytes()), "Grüße");
    }
}
