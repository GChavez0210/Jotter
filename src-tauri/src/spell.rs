//! Spell checking against Hunspell dictionaries with `spellbook`.
//!
//! Bundled dictionaries ship as app resources in `dictionaries/<id>/index.{aff,dic}`. Users add
//! more by dropping `<id>.aff` and `<id>.dic` into the app data `dictionaries` folder, which
//! also holds the personal word list. Nothing here reads or writes the vault.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use spellbook::Dictionary;

use crate::types::SpellDictionaryInfo;

/// Display names for the dictionaries bundled with Jotter, in the order Settings lists them.
pub const BUNDLED_DICTIONARIES: &[(&str, &str)] = &[
    ("en-US", "English (United States)"),
    ("en-GB", "English (United Kingdom)"),
    ("es", "Spanish"),
    ("fr", "French"),
    ("de", "German"),
    ("pt-BR", "Portuguese (Brazil)"),
    ("it", "Italian"),
];

const PERSONAL_FILE: &str = "personal.txt";
/// A suggestion menu longer than this stops being useful.
const MAX_SUGGESTIONS: usize = 8;

/// Folders the dictionaries load from.
#[derive(Debug, Clone)]
pub struct SpellPaths {
    /// The app's bundled `dictionaries` resource folder, if the app has one.
    pub bundled: Option<PathBuf>,
    /// the app data `dictionaries` folder: user dictionaries and the personal word list.
    pub user: PathBuf,
}

impl SpellPaths {
    fn files_for(&self, id: &str) -> Option<(PathBuf, PathBuf)> {
        if !is_safe_id(id) {
            return None;
        }
        // A user dictionary with a bundled id replaces the bundled one.
        let user = (self.user.join(format!("{id}.aff")), self.user.join(format!("{id}.dic")));
        if user.0.is_file() && user.1.is_file() {
            return Some(user);
        }
        let dir = self.bundled.as_ref()?.join(id);
        let bundled = (dir.join("index.aff"), dir.join("index.dic"));
        (bundled.0.is_file() && bundled.1.is_file()).then_some(bundled)
    }
}

/// Dictionary ids become file names, so they may hold only letters, digits, `-` and `_`.
fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Loaded dictionaries and the personal word list, shared by all spell commands.
#[derive(Default)]
pub struct SpellState {
    dictionaries: Mutex<HashMap<String, Arc<Dictionary>>>,
    /// Ids whose files failed to parse, so a broken user dictionary is not reparsed on every call.
    failed: Mutex<HashSet<String>>,
    personal: Mutex<Option<HashSet<String>>>,
}

impl SpellState {
    /// Returns the dictionaries for `ids`, loading any not yet in memory. Unknown ids are skipped.
    fn dictionaries(&self, paths: &SpellPaths, ids: &[String]) -> Result<Vec<Arc<Dictionary>>, String> {
        let mut out = Vec::new();
        // Serialize loading with reload so an old parse cannot repopulate a cleared cache.
        let mut loaded = self.lock_dictionaries()?;
        for id in ids {
            if let Some(dict) = loaded.get(id) {
                out.push(Arc::clone(dict));
                continue;
            }
            if self.failed.lock().is_ok_and(|failed| failed.contains(id)) {
                continue;
            }
            let Some((aff_path, dic_path)) = paths.files_for(id) else {
                continue;
            };
            // Commands run on a blocking worker; parsing does not block the window thread.
            let dict = match load_dictionary(&aff_path, &dic_path) {
                Ok(dict) => Arc::new(dict),
                Err(err) => {
                    // One broken dictionary must not stop checking in the note's other languages.
                    eprintln!("{err}");
                    if let Ok(mut failed) = self.failed.lock() {
                        failed.insert(id.clone());
                    }
                    continue;
                }
            };
            loaded.entry(id.clone())
                .or_insert_with(|| Arc::clone(&dict));
            out.push(dict);
        }
        Ok(out)
    }

    /// Reload after the user replaces dictionary files or edits personal.txt.
    pub fn reload(&self) {
        if let Ok(mut dictionaries) = self.dictionaries.lock() { dictionaries.clear(); }
        if let Ok(mut personal) = self.personal.lock() { *personal = None; }
        if let Ok(mut failed) = self.failed.lock() {
            failed.clear();
        }
    }

    fn lock_dictionaries(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, HashMap<String, Arc<Dictionary>>>, String> {
        self.dictionaries
            .lock()
            .map_err(|e| format!("Spell checker state is unavailable: {e}"))
    }

    fn with_personal<T>(&self, paths: &SpellPaths, f: impl FnOnce(&mut HashSet<String>) -> T) -> Result<T, String> {
        let mut guard = self
            .personal
            .lock()
            .map_err(|e| format!("Personal dictionary is unavailable: {e}"))?;
        let words = guard.get_or_insert_with(|| read_personal_words(&paths.user.join(PERSONAL_FILE)));
        Ok(f(words))
    }

    /// Returns the words that no dictionary in `languages` and no personal entry accepts.
    pub fn misspelled(&self, paths: &SpellPaths, languages: &[String], words: &[String]) -> Result<Vec<String>, String> {
        let dictionaries = self.dictionaries(paths, languages)?;
        if dictionaries.is_empty() {
            return Ok(Vec::new());
        }
        self.with_personal(paths, |personal| {
            words
                .iter()
                .filter(|word| {
                    let normalized = normalize_word(word);
                    !personal_accepts(personal, &normalized)
                        && !dictionaries.iter().any(|d| d.check(&normalized))
                })
                .cloned()
                .collect()
        })
    }

    /// Suggestions from every dictionary in `languages`, interleaved so each language is represented.
    pub fn suggest(&self, paths: &SpellPaths, languages: &[String], word: &str) -> Result<Vec<String>, String> {
        let normalized = normalize_word(word);
        let per_dictionary: Vec<Vec<String>> = self
            .dictionaries(paths, languages)?
            .iter()
            .map(|dict| {
                let mut found = Vec::new();
                dict.suggest(&normalized, &mut found);
                found
            })
            .collect();

        let mut seen = HashSet::new();
        let mut out = Vec::new();
        let longest = per_dictionary.iter().map(Vec::len).max().unwrap_or(0);
        for rank in 0..longest {
            for list in &per_dictionary {
                if let Some(s) = list.get(rank) {
                    if seen.insert(s.clone()) {
                        out.push(s.clone());
                    }
                }
            }
        }
        out.truncate(MAX_SUGGESTIONS);
        Ok(out)
    }

    /// Adds `word` to the personal word list and appends it to `personal.txt`.
    pub fn add_personal_word(&self, paths: &SpellPaths, word: &str) -> Result<(), String> {
        let word = normalize_word(word.trim());
        if word.is_empty() || word.chars().any(char::is_whitespace) {
            return Err("Only a single word can be added to the dictionary.".to_string());
        }
        self.with_personal(paths, |personal| -> Result<(), String> {
            if personal.contains(&word) { return Ok(()); }
            fs::create_dir_all(&paths.user)
                .map_err(|e| format!("Cannot create dictionaries folder {}: {e}", paths.user.display()))?;
            let path = paths.user.join(PERSONAL_FILE);
            let mut file = fs::OpenOptions::new().create(true).append(true).open(&path)
                .map_err(|e| format!("Cannot open {}: {e}", path.display()))?;
            writeln!(file, "{word}").map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
            personal.insert(word);
            Ok(())
        })?

    }
}

fn load_dictionary(aff_path: &Path, dic_path: &Path) -> Result<Dictionary, String> {
    let aff = fs::read_to_string(aff_path)
        .map_err(|e| format!("Cannot read {} as UTF-8: {e}", aff_path.display()))?;
    let dic = fs::read_to_string(dic_path)
        .map_err(|e| format!("Cannot read {} as UTF-8: {e}", dic_path.display()))?;
    Dictionary::new(&aff, &dic).map_err(|e| format!("Cannot parse dictionary {}: {e}", aff_path.display()))
}

fn read_personal_words(path: &Path) -> HashSet<String> {
    fs::read_to_string(path)
        .map(|text| {
            text.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Hunspell dictionaries spell apostrophes as `'`, but typographic quotes produce `’`.
fn normalize_word(word: &str) -> String {
    word.replace('\u{2019}', "'")
}

/// A lowercase entry also accepts the capitalized and all-caps forms, as Hunspell does.
fn personal_accepts(personal: &HashSet<String>, word: &str) -> bool {
    if personal.contains(word) {
        return true;
    }
    let lower = word.to_lowercase();
    lower != word && personal.contains(&lower)
}

/// Lists bundled dictionaries that are present, then user dictionaries, without loading any.
pub fn list_dictionaries(paths: &SpellPaths) -> Vec<SpellDictionaryInfo> {
    let mut out: Vec<SpellDictionaryInfo> = BUNDLED_DICTIONARIES
        .iter()
        .filter(|(id, _)| paths.files_for(id).is_some())
        .map(|(id, name)| SpellDictionaryInfo {
            id: (*id).to_string(),
            name: (*name).to_string(),
            bundled: true,
        })
        .collect();

    let mut user: Vec<SpellDictionaryInfo> = fs::read_dir(&paths.user)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let is_aff = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("aff"));
            let id = path.file_stem()?.to_str()?.to_string();
            let present = is_aff && is_safe_id(&id) && path.with_extension("dic").is_file();
            let is_bundled = BUNDLED_DICTIONARIES.iter().any(|(b, _)| *b == id);
            (present && !is_bundled).then(|| SpellDictionaryInfo {
                name: id.clone(),
                id,
                bundled: false,
            })
        })
        .collect();
    user.sort_by_key(|d| d.id.to_lowercase());
    out.extend(user);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const AFF: &str = "SET UTF-8\nTRY esianrtolcdugmphbyfvkwz'\n\nSFX S Y 1\nSFX S 0 s .\n";
    const DIC: &str = "4\nnote/S\nvault\ncolor\ndon't\n";
    const AFF_GB: &str = "SET UTF-8\n";
    const DIC_GB: &str = "1\ncolour\n";

    fn fixture() -> (tempfile::TempDir, SpellPaths) {
        let temp = tempfile::tempdir().unwrap();
        let bundled = temp.path().join("bundled");
        let user = temp.path().join("user");
        fs::create_dir_all(bundled.join("en-US")).unwrap();
        fs::write(bundled.join("en-US/index.aff"), AFF).unwrap();
        fs::write(bundled.join("en-US/index.dic"), DIC).unwrap();
        fs::create_dir_all(&user).unwrap();
        fs::write(user.join("en-GB.aff"), AFF_GB).unwrap();
        fs::write(user.join("en-GB.dic"), DIC_GB).unwrap();
        fs::write(user.join("nl.aff"), AFF_GB).unwrap();
        fs::write(user.join("nl.dic"), "1\nnotitie\n").unwrap();
        fs::write(user.join("orphan.aff"), AFF_GB).unwrap();
        let paths = SpellPaths { bundled: Some(bundled), user };
        (temp, paths)
    }

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn reports_words_no_selected_dictionary_accepts() {
        let (_temp, paths) = fixture();
        let state = SpellState::default();
        let misspelled = state
            .misspelled(&paths, &words(&["en-US"]), &words(&["notes", "vault", "vualt", "colour"]))
            .unwrap();
        assert_eq!(misspelled, words(&["vualt", "colour"]));
    }

    #[test]
    fn accepts_a_word_from_any_selected_dictionary() {
        let (_temp, paths) = fixture();
        let state = SpellState::default();
        let misspelled = state
            .misspelled(&paths, &words(&["en-US", "en-GB"]), &words(&["color", "colour", "kolor"]))
            .unwrap();
        assert_eq!(misspelled, words(&["kolor"]));
    }

    #[test]
    fn treats_typographic_apostrophes_like_straight_ones() {
        let (_temp, paths) = fixture();
        let state = SpellState::default();
        let misspelled = state
            .misspelled(&paths, &words(&["en-US"]), &words(&["don\u{2019}t", "don't"]))
            .unwrap();
        assert!(misspelled.is_empty());
    }

    #[test]
    fn checks_nothing_without_a_known_dictionary() {
        let (_temp, paths) = fixture();
        let state = SpellState::default();
        let misspelled = state
            .misspelled(&paths, &words(&["xx", "../../etc"]), &words(&["anything"]))
            .unwrap();
        assert!(misspelled.is_empty());
    }

    #[test]
    fn skips_a_dictionary_that_fails_to_parse() {
        let (_temp, paths) = fixture();
        fs::write(paths.user.join("bad.aff"), "SET UTF-8
").unwrap();
        fs::write(paths.user.join("bad.dic"), [0xff_u8, 0xfe, 0x00]).unwrap();
        let state = SpellState::default();
        let misspelled = state
            .misspelled(&paths, &words(&["bad", "en-US"]), &words(&["vault", "vualt"]))
            .unwrap();
        assert_eq!(misspelled, words(&["vualt"]));
    }

    #[test]
    fn suggests_corrections() {
        let (_temp, paths) = fixture();
        let state = SpellState::default();
        let suggestions = state.suggest(&paths, &words(&["en-US"]), "vualt").unwrap();
        assert_eq!(suggestions.first().map(String::as_str), Some("vault"));
    }

    #[test]
    fn personal_words_persist_and_cover_capitalized_forms() {
        let (_temp, paths) = fixture();
        let state = SpellState::default();
        state.add_personal_word(&paths, "jotter").unwrap();
        state.add_personal_word(&paths, "jotter").unwrap();
        assert_eq!(fs::read_to_string(paths.user.join(PERSONAL_FILE)).unwrap(), "jotter\n");

        let fresh = SpellState::default();
        let misspelled = fresh
            .misspelled(&paths, &words(&["en-US"]), &words(&["jotter", "Jotter", "JOTTER", "jottr"]))
            .unwrap();
        assert_eq!(misspelled, words(&["jottr"]));
        assert!(state.add_personal_word(&paths, "two words").is_err());
    }

    #[test]
    fn reload_reads_replaced_dictionary_and_personal_files() {
        let (_temp, paths) = fixture();
        let state = SpellState::default();
        assert!(state.misspelled(&paths, &words(&["nl"]), &words(&["notitie"])).unwrap().is_empty());
        fs::write(paths.user.join("nl.dic"), "1\nanders\n").unwrap();
        fs::write(paths.user.join(PERSONAL_FILE), "jotter\n").unwrap();
        state.reload();
        assert_eq!(state.misspelled(&paths, &words(&["nl"]), &words(&["notitie", "anders", "jotter"])).unwrap(), words(&["notitie"]));
    }

    #[test]
    fn failed_personal_write_can_be_retried() {
        let (_temp, paths) = fixture();
        let state = SpellState::default();
        fs::create_dir(paths.user.join(PERSONAL_FILE)).unwrap();
        assert!(state.add_personal_word(&paths, "jotter").is_err());
        assert_eq!(state.misspelled(&paths, &words(&["en-US"]), &words(&["jotter"])).unwrap(), words(&["jotter"]));
        fs::remove_dir(paths.user.join(PERSONAL_FILE)).unwrap();
        state.add_personal_word(&paths, "jotter").unwrap();
        assert_eq!(fs::read_to_string(paths.user.join(PERSONAL_FILE)).unwrap(), "jotter\n");
    }

    #[test]
    fn lists_bundled_then_user_dictionaries() {
        let (_temp, paths) = fixture();
        let listed: Vec<(String, bool)> = list_dictionaries(&paths)
            .into_iter()
            .map(|d| (d.id, d.bundled))
            .collect();
        // en-GB comes from the user folder but keeps its bundled slot and name.
        assert_eq!(
            listed,
            vec![("en-US".into(), true), ("en-GB".into(), true), ("nl".into(), false)]
        );
    }

    #[test]
    fn rejects_ids_that_could_escape_the_folders() {
        assert!(is_safe_id("pt-BR"));
        assert!(is_safe_id("en_AU"));
        assert!(!is_safe_id(""));
        assert!(!is_safe_id("../de"));
        assert!(!is_safe_id("de/index"));
    }

    /// Loads every bundled dictionary from `node_modules`, where `npm ci` puts them.
    #[test]
    fn bundled_dictionaries_parse() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../node_modules");
        let samples = [
            ("en", "en-US", "color", "kolor"),
            ("en-gb", "en-GB", "colour", "kolour"),
            ("es", "es", "canción", "cancion"),
            ("fr", "fr", "français", "francais"),
            ("de", "de", "Straße", "Strase"),
            ("pt", "pt-BR", "coração", "coracao"),
            ("it", "it", "perché", "perche"),
        ];
        for (package, id, good, bad) in samples {
            let dir = root.join(format!("dictionary-{package}"));
            if !dir.is_dir() {
                eprintln!("skipping {id}: run npm ci to install dictionary-{package}");
                continue;
            }
            let dict = load_dictionary(&dir.join("index.aff"), &dir.join("index.dic")).unwrap();
            assert!(dict.check(good), "{id} rejects {good}");
            assert!(!dict.check(bad), "{id} accepts {bad}");
        }
    }
}
