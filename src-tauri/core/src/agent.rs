//! The operations the `jotter` command-line tool and its MCP server expose to agents.
//!
//! Each call works on the vault files directly and answers with JSON. Reads build a throwaway
//! index in memory from the files on disk, so they never touch the app's own index and never
//! fail because the app has the database open. Writes go through the same atomic write path the
//! app uses, keep the text they replace in the note's version history first, and send deletes
//! to the Recycle Bin. A read-only handle refuses every write.

use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::SystemTime;

use rusqlite::Connection;
use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::ai_access::{self, AccessRules};
use crate::index::{self, system_time_to_ms};
use crate::search::VaultSearchIndex;
use crate::vault;
use crate::versions;

pub struct AgentVault {
    root: PathBuf,
    /// The root as the app spells it, which names the vault's version history and AI selection.
    history_root: PathBuf,
    read_only: bool,
    /// Limits every call to the notes the user shared with AI assistants.
    shared_only: bool,
    versions_dir: PathBuf,
    app_data_dir: PathBuf,
    /// The last search index and note index, with the fingerprint of the files they were built
    /// from. A long-lived MCP server reuses them until a file changes.
    search_cache: RefCell<Option<(u64, VaultSearchIndex)>>,
    index_cache: RefCell<Option<(u64, Rc<Connection>)>>,
}

fn fingerprint(files: &[index::ScannedNoteFile]) -> u64 {
    let mut entries: Vec<_> = files.iter().collect();
    entries.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    let mut hasher = DefaultHasher::new();
    for file in entries {
        (&file.path, file.fs_modified_ms, file.created_ms, &file.content).hash(&mut hasher);
    }
    hasher.finish()
}

fn to_json<T: Serialize>(value: &T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|e| format!("Cannot encode the result: {e}"))
}

fn now_ms() -> i64 {
    system_time_to_ms(SystemTime::now())
}

impl AgentVault {
    /// Opens the vault at `root`. `app_data_dir` holds the version history, so a snapshot taken
    /// here appears in the app's timeline for the same note.
    pub fn open(root: &Path, read_only: bool, app_data_dir: &Path) -> Result<Self, String> {
        // Preserve the path spelling the app uses for history. Canonicalization can expand
        // Windows short names or macOS /var aliases into a different persistence key.
        let history_root = std::path::absolute(root)
            .map_err(|e| format!("Cannot resolve the vault '{}': {e}", root.display()))?;
        let versions_dir = versions::versions_dir(app_data_dir, &history_root);
        let root = root
            .canonicalize()
            .map_err(|e| format!("Cannot open the vault '{}': {e}", root.display()))?;
        if !root.is_dir() {
            return Err(format!("'{}' is not a folder", root.display()));
        }
        Ok(Self {
            root,
            history_root,
            read_only,
            shared_only: false,
            versions_dir,
            app_data_dir: app_data_dir.to_path_buf(),
            search_cache: RefCell::new(None),
            index_cache: RefCell::new(None),
        })
    }

    /// Hides every note the user has not shared with AI assistants, and allows writes only in
    /// the AI folder.
    pub fn shared_only(mut self) -> Self {
        self.shared_only = true;
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The selection to enforce, or `None` when this handle may read the whole vault. It is
    /// loaded for each call, so a change in the app applies to a running server.
    fn rules(&self) -> Result<Option<AccessRules>, String> {
        self.shared_only
            .then(|| AccessRules::load(&self.app_data_dir, &self.history_root))
            .transpose()
    }

    fn scan(&self) -> Result<Vec<index::ScannedNoteFile>, String> {
        let mut files = index::scan_dir_files_from_disk(&self.root, &self.root)?;
        if let Some(rules) = self.rules()? {
            files.retain(|file| is_shared(&rules, &file.path, &file.content));
        }
        Ok(files)
    }

    /// The path to read for a note, folder, or attachment. A shared-only handle refuses paths
    /// the user has not shared.
    fn permit(&self, path: &str) -> Result<String, String> {
        let (_, clean) = vault::resolve_safe_vault_path(&self.root, path)?;
        let Some(rules) = self.rules()? else {
            return Ok(clean);
        };
        let spelled = ai_access::disk_spelling(&self.root, &clean).ok_or_else(|| not_shared(&clean))?;
        if !rules.can_read(&spelled) || self.is_blocked(&spelled) {
            return Err(not_shared(&clean));
        }
        Ok(spelled)
    }

    /// The path to change or create. A shared-only handle allows only the AI folder.
    fn permit_write(&self, path: &str) -> Result<String, String> {
        let (_, clean) = vault::resolve_safe_vault_path(&self.root, path)?;
        let Some(rules) = self.rules()? else {
            return Ok(clean);
        };
        let spelled = ai_access::disk_spelling(&self.root, &clean).ok_or_else(|| not_shared(&clean))?;
        rules.check_write(&spelled)?;
        if self.is_blocked(&spelled) {
            return Err(not_shared(&clean));
        }
        Ok(spelled)
    }

    /// Whether the note at `path` says `ai: false`, or cannot be read to find out.
    fn is_blocked(&self, path: &str) -> bool {
        let abs = self.root.join(path);
        abs.is_file()
            && path.to_ascii_lowercase().ends_with(".md")
            && std::fs::read(&abs).map_or(true, |bytes| ai_access::blocks_ai(&String::from_utf8_lossy(&bytes)))
    }

    /// Refuses to move or delete a folder that holds a note the caller may not change.
    fn permit_contents(&self, clean: &str) -> Result<(), String> {
        let Some(rules) = self.rules()? else {
            return Ok(());
        };
        let abs = self.root.join(clean);
        if abs.is_dir()
            && index::scan_dir_files_from_disk(&self.root, &abs)?
                .iter()
                .any(|file| rules.check_write(&file.path).is_err() || ai_access::blocks_ai(&file.content))
        {
            return Err(format!(
                "'{clean}' holds notes that assistants may not change, so it cannot be moved or deleted"
            ));
        }
        Ok(())
    }

    fn writable(&self) -> Result<(), String> {
        if self.read_only {
            Err("This vault is open read-only, so writes are refused".to_string())
        } else {
            Ok(())
        }
    }

    fn write_lock(&self) -> Result<crate::vault_lock::VaultWriteLock, String> {
        self.writable()?;
        crate::vault_lock::VaultWriteLock::acquire(&self.app_data_dir, &self.root)
    }

    fn index(&self) -> Result<Rc<Connection>, String> {
        let files = self.scan()?;
        let print = fingerprint(&files);
        if let Some((cached, conn)) = self.index_cache.borrow().as_ref() {
            if *cached == print {
                return Ok(conn.clone());
            }
        }
        let conn = Connection::open_in_memory().map_err(|e| format!("Cannot build the index: {e}"))?;
        index::apply_scanned_vault_index(&conn, &files)?;
        let conn = Rc::new(conn);
        *self.index_cache.borrow_mut() = Some((print, conn.clone()));
        Ok(conn)
    }

    /// Keeps the text a write is about to replace, even when the last snapshot is recent.
    fn keep_previous(&self, rel_path: &str, previous: &str) {
        // A failed snapshot must never stop the write itself, as in the app.
        let _ = versions::snapshot(&self.versions_dir, rel_path, previous, now_ms(), 0);
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Value, String> {
        let files = self.scan()?;
        let print = fingerprint(&files);
        let mut cache = self.search_cache.borrow_mut();
        if cache.as_ref().is_none_or(|(cached, _)| *cached != print) {
            let search = VaultSearchIndex::new()?;
            search.rebuild_from_scanned(&files)?;
            *cache = Some((print, search));
        }
        let (_, search) = cache.as_ref().expect("the cache was just filled");
        let hits = search.search(query, limit.clamp(1, 200))?;
        to_json(&hits)
    }

    /// Every note with its metadata and link counts, optionally only those under `folder`.
    pub fn list(&self, folder: Option<&str>) -> Result<Value, String> {
        let mut cards = index::query_note_cards(&*self.index()?)?;
        if let Some(folder) = folder {
            let folder = vault::normalize_rel_path(folder);
            if !folder.is_empty() {
                let (_, clean) = vault::resolve_safe_vault_path(&self.root, &folder)?;
                let prefix = format!("{clean}/");
                cards.retain(|card| card.path.starts_with(&prefix));
            }
        }
        to_json(&cards)
    }

    pub fn read(&self, path: &str) -> Result<Value, String> {
        let clean = self.permit(&vault::ensure_md_extension(path))?;
        let note = vault::read_vault_note(&self.root, &clean)?;
        // Checked again on the text being returned, which an editor may have just changed.
        if self.shared_only && ai_access::blocks_ai(&note.content) {
            return Err(not_shared(&clean));
        }
        to_json(&note)
    }

    pub fn create(&self, path: &str, content: &str) -> Result<Value, String> {
        let _write_lock = self.write_lock()?;
        self.create_locked(path, content)
    }

    fn create_locked(&self, path: &str, content: &str) -> Result<Value, String> {
        let path = self.permit_write(&vault::ensure_md_extension(path))?;
        let (_, doc) = vault::create_vault_note(&self.root, &path, content)?;
        to_json(&doc)
    }

    /// Adds `text` to the end of a note on its own line. With `create_missing`, a note that does
    /// not exist yet is created instead of reported as an error.
    pub fn append(&self, path: &str, text: &str, create_missing: bool) -> Result<Value, String> {
        let _write_lock = self.write_lock()?;
        self.append_locked(path, text, create_missing)
    }

    fn append_locked(&self, path: &str, text: &str, create_missing: bool) -> Result<Value, String> {
        let clean = self.permit_write(&vault::ensure_md_extension(path))?;
        let abs = self.root.join(&clean);
        if !abs.exists() {
            if !create_missing {
                return Err(format!("Note does not exist: {clean}"));
            }
            return self.create_locked(&clean, &with_final_newline(text));
        }
        let previous = std::fs::read_to_string(&abs).map_err(|e| format!("Cannot read {clean}: {e}"))?;
        let separator = if previous.is_empty() || previous.ends_with('\n') { "" } else { "\n" };
        let updated = format!("{previous}{separator}{}", with_final_newline(text));
        self.keep_previous(&clean, &previous);
        let (doc, _) = vault::save_vault_note(&self.root, &clean, &updated)?;
        to_json(&doc)
    }

    /// Renames or moves a note or folder. For a note, links that point to it are rewritten
    /// unless `update_links` is false; each rewritten note keeps its old text in version history.
    pub fn rename(&self, from: &str, to: &str, update_links: bool) -> Result<Value, String> {
        let _write_lock = self.write_lock()?;
        self.rename_locked(from, to, update_links)
    }

    /// The caller holds the vault write lock across a larger operation.
    pub(crate) fn rename_locked(&self, from: &str, to: &str, update_links: bool) -> Result<Value, String> {
        let from = &self.existing_entry(from)?;
        self.permit_contents(from)?;
        let to = &self.permit_destination(from, to)?;
        let plan = if update_links {
            crate::link_update::plan_link_updates(&self.root, from, to)?
        } else {
            Default::default()
        };
        let renamed = vault::rename_vault_entry(&self.root, from, to)?;
        versions::rename(&self.versions_dir, &renamed.old_rel_path, &renamed.new_rel_path)?;
        let moved = AccessRules::update(&self.app_data_dir, &self.history_root, |rules| {
            rules.rename(&renamed.old_rel_path, &renamed.new_rel_path)
        });
        // The files have moved, so a selection that cannot be saved does not fail the rename.
        let rules = moved.unwrap_or_else(|error| {
            eprintln!("Cannot move the AI note selection: {error}");
            AccessRules::default()
        });
        let mut links_updated = 0;
        let mut notes_updated = 0;
        let mut links_skipped = 0;
        for note in &plan.notes {
            let (abs, clean) = vault::resolve_safe_vault_path(&self.root, &note.path)?;
            if self.permit_write(&clean).is_err() {
                // Counted only for notes the assistant can see.
                if std::fs::read_to_string(&abs).is_ok_and(|text| is_shared(&rules, &clean, &text)) {
                    links_skipped += note.edits.len();
                }
                continue;
            }
            if let Ok(previous) = std::fs::read_to_string(&abs) {
                self.keep_previous(&clean, &previous);
            }
            let applied = crate::link_update::apply_edits_to_note(&self.root, &clean, &note.edits)?.applied;
            if applied > 0 {
                links_updated += applied;
                notes_updated += 1;
            }
        }
        Ok(json!({
            "oldPath": renamed.old_rel_path,
            "newPath": renamed.new_rel_path,
            "linksUpdated": links_updated,
            "notesUpdated": notes_updated,
            "linksSkipped": links_skipped,
        }))
    }

    /// Sends a note or folder to the Recycle Bin.
    pub fn delete(&self, path: &str) -> Result<Value, String> {
        let _write_lock = self.write_lock()?;
        let clean = self.existing_entry(path)?;
        self.permit_contents(&clean)?;
        vault::delete_vault_entry(&self.root, &clean)?;
        if let Err(error) = AccessRules::update(&self.app_data_dir, &self.history_root, |rules| rules.forget(&clean)) {
            eprintln!("Cannot update the AI note selection: {error}");
        }
        Ok(json!({ "deleted": clean }))
    }

    /// The vault-relative path of an existing note or folder. A name without `.md` finds the note.
    fn existing_entry(&self, path: &str) -> Result<String, String> {
        let (abs, clean) = vault::resolve_safe_vault_path(&self.root, path)?;
        if abs.exists() {
            let permitted = self.permit_write(&clean)?;
            if self.shared_only && permitted == ai_access::AI_FOLDER {
                return Err("The AI folder itself cannot be moved or deleted".to_string());
            }
            return Ok(permitted);
        }
        let (note_abs, note_clean) = vault::resolve_safe_vault_path(&self.root, &vault::ensure_md_extension(path))?;
        if note_abs.is_file() {
            self.permit_write(&note_clean)
        } else {
            self.permit_write(&clean)?;
            Err(format!("Nothing exists at '{clean}'"))
        }
    }

    /// Where a rename may put `from`.
    fn permit_destination(&self, from: &str, to: &str) -> Result<String, String> {
        if !self.shared_only {
            return Ok(to.to_string());
        }
        let is_note = self.root.join(from).is_file() && from.to_ascii_lowercase().ends_with(".md");
        self.permit_write(&if is_note { vault::ensure_md_extension(to) } else { to.to_string() })
    }

    pub fn backlinks(&self, path: &str) -> Result<Value, String> {
        let clean = self.permit(&vault::ensure_md_extension(path))?;
        to_json(&index::query_backlinks_for_note(&*self.index()?, &clean)?)
    }

    /// All tags with their note counts, or the notes carrying `tag`.
    pub fn tags(&self, tag: Option<&str>) -> Result<Value, String> {
        let conn = self.index()?;
        match tag {
            Some(tag) => to_json(&index::query_notes_by_tag(&conn, tag.trim_start_matches('#'))?),
            None => to_json(&index::query_all_tags(&conn)?),
        }
    }

    /// Checklist items that have a due date.
    pub fn tasks(&self) -> Result<Value, String> {
        to_json(&index::query_due_tasks(&*self.index()?)?)
    }

    pub fn health(&self) -> Result<Value, String> {
        let Some(rules) = self.rules()? else {
            return to_json(&crate::health::scan_vault_health(&self.root, "")?);
        };
        let notes = index::scan_dir_files_from_disk(&self.root, &self.root)?;
        let attachments = crate::health::collect_attachments(&self.root)?;
        let shared: HashSet<&str> = notes
            .iter()
            .filter(|file| is_shared(&rules, &file.path, &file.content))
            .map(|file| file.path.as_str())
            .collect();
        let visible = |path: &str| {
            if path.to_ascii_lowercase().ends_with(".md") {
                shared.contains(path)
            } else {
                rules.can_read(path)
            }
        };
        let notes: Vec<_> = notes.iter().filter(|note| visible(&note.path)).cloned().collect();
        let attachments: Vec<_> = attachments.iter().filter(|path| visible(path)).cloned().collect();
        to_json(&crate::health::build_report(&notes, &attachments, ""))
    }

    /// The stored versions of a note, newest first.
    pub fn versions(&self, path: &str) -> Result<Value, String> {
        let (_, clean) = versions::resolve_note_path(&self.root, &self.permit_note_path(path)?)?;
        let mut listed = versions::list(&self.versions_dir, &clean);
        if self.shared_only {
            listed.retain(|version| versions::read(&self.versions_dir, &clean, &version.id)
                .is_ok_and(|content| !ai_access::blocks_ai(&content)));
        }
        to_json(&listed)
    }

    pub fn version(&self, path: &str, id: &str) -> Result<Value, String> {
        let (_, clean) = versions::resolve_note_path(&self.root, &self.permit_note_path(path)?)?;
        let content = versions::read(&self.versions_dir, &clean, id)
            .map_err(|error| if self.shared_only { not_shared(&clean) } else { error })?;
        if self.shared_only && ai_access::blocks_ai(&content) {
            return Err(not_shared(&clean));
        }
        Ok(json!({ "path": clean, "id": id, "content": content }))
    }

    fn permit_note_path(&self, path: &str) -> Result<String, String> {
        if self.shared_only {
            self.permit(&vault::ensure_md_extension(path))
        } else {
            Ok(path.to_string())
        }
    }

    /// The shared context note every assistant reads first, and the other notes in its folder.
    pub fn context(&self) -> Result<Value, String> {
        let content = self.readable_text(CONTEXT_NOTE)?;
        let instructions = self.readable_text(AGENTS_NOTE)?;
        let set_up = self.rules()?.is_none_or(|rules| rules.ai_folder);
        let prefix = format!("{CONTEXT_FOLDER}/");
        let conversations_prefix = format!("{CONVERSATIONS_FOLDER}/");
        let cards = index::query_note_cards(&*self.index()?)?;
        let others: Vec<&str> = cards
            .iter()
            .map(|card| card.path.as_str())
            .filter(|path| {
                path.starts_with(&prefix)
                    && *path != CONTEXT_NOTE
                    && *path != AGENTS_NOTE
                    // Logs and results pile up; `list_notes` finds them when needed.
                    && ![CONVERSATIONS_FOLDER, DAILY_LOG_FOLDER, OUTPUTS_FOLDER]
                        .iter()
                        .any(|folder| path.strip_prefix(folder).is_some_and(|rest| rest.starts_with('/')))
            })
            .collect();
        let mut conversations: Vec<_> = cards.iter().filter(|card| card.path.starts_with(&conversations_prefix)).collect();
        conversations.sort_by_key(|card| std::cmp::Reverse(card.modified_ms));
        let recent: Vec<Value> = conversations
            .iter()
            .take(RECENT_CONVERSATIONS)
            .map(|card| json!({ "path": card.path, "title": card.title, "modifiedMs": card.modified_ms }))
            .collect();
        Ok(json!({
            "path": CONTEXT_NOTE,
            "exists": content.is_some(),
            "content": content.unwrap_or_default(),
            "otherNotes": others,
            "setUp": set_up,
            "instructions": instructions.unwrap_or_default(),
            "recentConversations": recent,
        }))
    }

    /// The text of a note, or `None` when it is missing or the caller may not read it.
    fn readable_text(&self, path: &str) -> Result<Option<String>, String> {
        let (abs, _) = vault::resolve_safe_vault_path(&self.root, path)?;
        if !abs.exists() || (self.shared_only && self.permit(path).is_err()) {
            return Ok(None);
        }
        Ok(Some(vault::read_vault_note(&self.root, path)?.content))
    }

    /// Replaces `old_text` with `new_text` in a note. `old_text` must appear exactly once unless
    /// `replace_all` is set, so an edit never lands somewhere the caller did not mean. The note's
    /// previous text stays in version history.
    pub fn edit(&self, path: &str, old_text: &str, new_text: &str, replace_all: bool) -> Result<Value, String> {
        let _write_lock = self.write_lock()?;
        if old_text.is_empty() {
            return Err("old_text is empty. Give the exact text to replace".to_string());
        }
        let clean = self.existing_note(path)?;
        let (abs, _) = vault::resolve_safe_vault_path(&self.root, &clean)?;
        let previous = std::fs::read_to_string(&abs).map_err(|e| format!("Cannot read {clean}: {e}"))?;
        // A note saved with Windows line endings still matches text written with "\n".
        let (old, new) = if !previous.contains(old_text) && previous.contains("\r\n") {
            (old_text.replace('\n', "\r\n"), new_text.replace('\n', "\r\n"))
        } else {
            (old_text.to_string(), new_text.to_string())
        };
        let count = previous.matches(&old).count();
        match count {
            0 => return Err(format!("old_text does not appear in {clean}. Read the note and copy the text exactly")),
            1 => {}
            _ if !replace_all => {
                return Err(format!(
                    "old_text appears {count} times in {clean}. Add surrounding text to make it unique, or set replace_all"
                ))
            }
            _ => {}
        }
        let updated = previous.replace(&old, &new);
        self.keep_previous(&clean, &previous);
        vault::save_vault_note(&self.root, &clean, &updated)?;
        Ok(json!({ "path": clean, "replacements": count }))
    }

    /// Sets or removes frontmatter properties. A `null` value removes that property.
    pub fn set_properties(&self, path: &str, set: &Map<String, Value>, remove: &[String]) -> Result<Value, String> {
        let _write_lock = self.write_lock()?;
        let clean = self.existing_note(path)?;
        self.rewrite(&clean, |text| crate::frontmatter::set_properties(text, set, remove))?;
        Ok(json!({ "path": clean }))
    }

    /// Adds and removes tags in the note's frontmatter `tags` list. Tags written as `#tag` in the
    /// body are left alone, so the answer lists any that keep a removed tag on the note.
    pub fn update_tags(&self, path: &str, add: &[String], remove: &[String]) -> Result<Value, String> {
        let _write_lock = self.write_lock()?;
        let clean = self.existing_note(path)?;
        let normalize = |tag: &String| tag.trim().trim_start_matches('#').to_string();
        let add: Vec<String> = add.iter().map(normalize).collect();
        let remove: Vec<String> = remove.iter().map(normalize).collect();
        for tag in &add {
            let valid = !tag.is_empty()
                && tag.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '/')
                && tag.chars().any(|c| !c.is_numeric());
            if !valid {
                return Err(format!("'{tag}' is not a valid tag. Use letters, digits, _, -, and /, with at least one non-digit"));
            }
        }
        let mut tags = Vec::new();
        self.rewrite(&clean, |text| {
            let inner = crate::frontmatter::split(text).map(|(inner, _)| inner).unwrap_or("");
            let tag_keys: Vec<String> = inner.lines().filter_map(crate::frontmatter::key_of)
                .filter(|key| key.eq_ignore_ascii_case("tag") || key.eq_ignore_ascii_case("tags"))
                .collect();
            let normalized = inner.lines().map(|line| {
                if crate::frontmatter::key_of(line).is_some_and(|key| tag_keys.contains(&key)) {
                    format!("tags:{}", line.split_once(':').map(|(_, value)| value).unwrap_or_default())
                } else {
                    line.to_string()
                }
            }).collect::<Vec<_>>().join("\n");
            tags = crate::links::extract_frontmatter_tags(&normalized);
            tags.retain(|t| !remove.iter().any(|r| r.eq_ignore_ascii_case(t)));
            for tag in &add {
                if !tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                    tags.push(tag.clone());
                }
            }
            let mut set = Map::new();
            set.insert("tags".to_string(), if tags.is_empty() { Value::Null } else { json!(tags) });
            // Fold singular and differently cased spellings into the existing canonical key.
            let aliases: Vec<String> = tag_keys.into_iter().filter(|key| key != "tags").collect();
            crate::frontmatter::set_properties(text, &set, &aliases)
        })?;
        let content = std::fs::read_to_string(vault::resolve_safe_vault_path(&self.root, &clean)?.0).unwrap_or_default();
        let body = crate::frontmatter::split(&content).map(|(_, body)| body).unwrap_or(&content);
        let body_tags = crate::links::extract_tags(body);
        let still_in_body: Vec<&String> = remove.iter().filter(|r| body_tags.iter().any(|t| t.eq_ignore_ascii_case(r))).collect();
        Ok(json!({ "path": clean, "tags": tags, "stillInBody": still_in_body }))
    }

    /// Saves notes from a conversation to `AI/Conversations/<date> <title>.md`. A later call
    /// with the same title appends to that note, even on another day, so one conversation keeps
    /// one note.
    pub fn save_conversation(&self, title: &str, text: &str) -> Result<Value, String> {
        let _write_lock = self.write_lock()?;
        let title = conversation_title(title);
        let suffix = format!(" {}.md", title.to_lowercase());
        let (folder_abs, _) = vault::resolve_safe_vault_path(&self.root, CONVERSATIONS_FOLDER)?;
        let existing = std::fs::read_dir(&folder_abs)
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
            .filter(|name| {
                let lower = name.to_lowercase();
                lower.ends_with(&suffix) && lower.len() == suffix.len() + 10 && is_iso_date(&lower[..10])
            })
            .max();
        if let Some(name) = existing {
            let path = format!("{CONVERSATIONS_FOLDER}/{name}");
            self.append_locked(&path, &format!("\n{}", text.trim_end()), false)?;
            return Ok(json!({ "path": path, "created": false }));
        }
        let path = format!("{CONVERSATIONS_FOLDER}/{} {title}.md", chrono::Local::now().format("%Y-%m-%d"));
        self.create_locked(&path, &format!("# {title}\n\n{}", with_final_newline(text.trim_end())))?;
        Ok(json!({ "path": path, "created": true }))
    }

    /// Adds `text` to the end of today's daily note, creating the note the way the app does:
    /// in the daily folder from Settings, from the daily template, named `YYYY-MM-DD.md`. A
    /// shared-only handle writes to the AI folder's log for today instead.
    pub fn append_to_daily(&self, text: &str) -> Result<Value, String> {
        let _write_lock = self.write_lock()?;
        if self.shared_only {
            let date = chrono::Local::now().format("%Y-%m-%d").to_string();
            let clean = self.permit_write(&format!("{DAILY_LOG_FOLDER}/{date}.md"))?;
            let created = !self.root.join(&clean).exists();
            if created {
                self.create_locked(&clean, &format!("# {date}\n\n"))?;
            }
            self.append_locked(&clean, text, false)?;
            return Ok(json!({ "path": clean, "created": created }));
        }
        let settings = self.app_settings();
        let setting = |key: &str, default: &str| {
            settings.get(key).and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| default.to_string())
        };
        let folder = setting("jotter.dailyFolder", "Daily").trim_matches('/').to_string();
        let now = chrono::Local::now();
        let date = now.format("%Y-%m-%d").to_string();
        let path = if folder.is_empty() { format!("{date}.md") } else { format!("{folder}/{date}.md") };
        let (abs, clean) = vault::resolve_safe_vault_path(&self.root, &path)?;
        let created = !abs.exists();
        if created {
            let template_path = setting("jotter.dailyTemplatePath", "Templates/Daily.md");
            let template = (!template_path.is_empty())
                .then(|| vault::read_vault_note(&self.root, &vault::ensure_md_extension(&template_path)).ok())
                .flatten()
                .map(|doc| doc.content)
                .unwrap_or_else(|| DEFAULT_DAILY_TEMPLATE.to_string());
            let filled = template
                .replace("{{date}}", &date)
                .replace("{{time}}", &now.format("%H:%M").to_string())
                .replace("{{title}}", &date);
            self.create_locked(&clean, &filled)?;
        }
        self.append_locked(&clean, text, false)?;
        Ok(json!({ "path": clean, "created": created }))
    }

    /// The app's settings map, or an empty one when the app has not saved settings.
    fn app_settings(&self) -> Map<String, Value> {
        std::fs::read_to_string(self.app_data_dir.join("settings.json"))
            .ok()
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default()
    }

    /// The vault path of an existing note. A name without `.md` finds the note.
    fn existing_note(&self, path: &str) -> Result<String, String> {
        let clean = self.permit_write(&vault::ensure_md_extension(path))?;
        if self.root.join(&clean).is_file() {
            Ok(clean)
        } else {
            Err(format!("Note does not exist: {clean}"))
        }
    }

    /// Rewrites a note with `change`, keeping the old text in version history. A change that
    /// leaves the text as it was writes nothing.
    fn rewrite(&self, clean: &str, change: impl FnOnce(&str) -> Result<String, String>) -> Result<(), String> {
        let (abs, _) = vault::resolve_safe_vault_path(&self.root, clean)?;
        let previous = std::fs::read_to_string(&abs).map_err(|e| format!("Cannot read {clean}: {e}"))?;
        let updated = change(&previous)?;
        if updated != previous {
            self.keep_previous(clean, &previous);
            vault::save_vault_note(&self.root, clean, &updated)?;
        }
        Ok(())
    }

    /// Replaces the context note. The text it replaces stays in version history.
    pub fn set_context(&self, text: &str) -> Result<Value, String> {
        let _write_lock = self.write_lock()?;
        self.permit_write(CONTEXT_NOTE)?;
        let (abs, _) = vault::resolve_safe_vault_path(&self.root, CONTEXT_NOTE)?;
        let text = with_final_newline(text);
        if !abs.exists() {
            return self.create_locked(CONTEXT_NOTE, &text);
        }
        let previous = std::fs::read_to_string(&abs).map_err(|e| format!("Cannot read {CONTEXT_NOTE}: {e}"))?;
        self.keep_previous(CONTEXT_NOTE, &previous);
        let (doc, _) = vault::save_vault_note(&self.root, CONTEXT_NOTE, &text)?;
        to_json(&doc)
    }
}

const CONTEXT_FOLDER: &str = ai_access::AI_FOLDER;
const CONTEXT_NOTE: &str = "AI/Context.md";
const AGENTS_NOTE: &str = "AI/AGENTS.md";
const CONVERSATIONS_FOLDER: &str = "AI/Conversations";
const DAILY_LOG_FOLDER: &str = "AI/Daily";
const OUTPUTS_FOLDER: &str = "AI/Outputs";
const RECENT_CONVERSATIONS: usize = 5;
/// The app's built-in daily template, used when the configured template note is missing.
const DEFAULT_DAILY_TEMPLATE: &str = "## Today\n\n- [ ] \n\n## Log\n\n";

/// A conversation title made safe for a file name on every system: no path or reserved
/// characters, no leading or trailing dots, at most 80 characters.
fn conversation_title(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| if c.is_control() || "\\/:*?\"<>|#^[]".contains(c) { ' ' } else { c })
        .collect();
    let words: Vec<&str> = cleaned.split_whitespace().collect();
    let mut title: String = words.join(" ").chars().take(80).collect();
    title = title.trim_matches(|c: char| c == '.' || c.is_whitespace()).to_string();
    if title.is_empty() { "Conversation".to_string() } else { title }
}

fn is_iso_date(text: &str) -> bool {
    chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d").is_ok()
}

fn is_shared(rules: &AccessRules, path: &str, content: &str) -> bool {
    rules.can_read(path) && !ai_access::blocks_ai(content)
}

/// One message for a hidden note and a missing one, so a caller cannot probe for names.
fn not_shared(path: &str) -> String {
    format!("'{path}' is not shared with AI assistants. The user chooses shared notes in Jotter under Settings > AI; notes with `ai: false` are never shared.")
}

fn with_final_newline(text: &str) -> String {
    if text.ends_with('\n') { text.to_string() } else { format!("{text}\n") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault_with_notes() -> (tempfile::TempDir, tempfile::TempDir) {
        let vault = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(vault.path().join("Sub")).unwrap();
        std::fs::write(vault.path().join("Home.md"), "# Home\nSee [[Plan]] #work\n- [ ] Pay 📅 2026-11-01\n").unwrap();
        std::fs::write(vault.path().join("Plan.md"), "The plan about oranges").unwrap();
        (vault, tempfile::tempdir().unwrap())
    }

    fn open(vault: &tempfile::TempDir, data: &tempfile::TempDir, read_only: bool) -> AgentVault {
        AgentVault::open(vault.path(), read_only, data.path()).unwrap()
    }

    fn share(vault: &tempfile::TempDir, data: &tempfile::TempDir, path: &str, allowed: bool) {
        AccessRules::update(data.path(), vault.path(), |rules| rules.set(path, allowed)).unwrap();
    }

    #[test]
    fn a_shared_only_handle_sees_nothing_until_notes_are_selected() {
        let (vault, data) = vault_with_notes();
        let agent = open(&vault, &data, false).shared_only();
        assert!(agent.list(None).unwrap().as_array().unwrap().is_empty());
        assert!(agent.search("oranges", 10).unwrap().as_array().unwrap().is_empty());
        assert!(agent.read("Plan").unwrap_err().contains("not shared"));
        share(&vault, &data, "Plan.md", true);
        assert_eq!(agent.read("Plan").unwrap()["content"], "The plan about oranges");
        assert_eq!(agent.list(None).unwrap().as_array().unwrap().len(), 1);
        // Home links to Plan but is not shared, so nothing from it appears.
        assert!(agent.backlinks("Plan").unwrap().as_array().unwrap().is_empty());
        assert!(agent.tags(None).unwrap().as_array().unwrap().is_empty());
        assert!(agent.tasks().unwrap().as_array().unwrap().is_empty());
    }

    fn set_up_ai(vault: &tempfile::TempDir, data: &tempfile::TempDir) {
        crate::ai_folder::set_up(data.path(), vault.path()).unwrap();
    }

    #[test]
    fn hidden_notes_do_not_change_the_visible_health_report() {
        let (vault, data) = vault_with_notes();
        share(&vault, &data, "Home.md", true);
        let agent = open(&vault, &data, false).shared_only();
        let with_hidden = agent.health().unwrap();
        std::fs::remove_file(vault.path().join("Plan.md")).unwrap();
        assert_eq!(with_hidden, agent.health().unwrap());
    }

    #[test]
    fn blocked_historical_versions_do_not_expose_metadata() {
        let (vault, data) = vault_with_notes();
        let agent = open(&vault, &data, false).shared_only();
        share(&vault, &data, "Plan.md", true);
        versions::snapshot(&agent.versions_dir, "Plan.md", "---\nai: false\n---\nprivate past", 1000, 0).unwrap();
        versions::snapshot(&agent.versions_dir, "Plan.md", "public past", 2000, 0).unwrap();
        let listed = agent.versions("Plan").unwrap();
        assert_eq!(listed.as_array().unwrap().len(), 1);
        assert_eq!(listed[0]["id"], "2000");
        assert_eq!(agent.version("Plan", "1000").unwrap_err(), agent.version("Plan", "9999").unwrap_err());
        assert_eq!(open(&vault, &data, false).versions("Plan").unwrap().as_array().unwrap().len(), 2);
    }

    #[test]
    fn hidden_notes_cannot_be_read_or_changed_through_any_call() {
        let (vault, data) = vault_with_notes();
        std::fs::write(vault.path().join("Sub/Secret.md"), "---\nai: false\n---\nzebra [[Gone]]").unwrap();
        set_up_ai(&vault, &data);
        share(&vault, &data, "", true);
        share(&vault, &data, "Home.md", false);
        let agent = open(&vault, &data, false).shared_only();
        for path in ["Home", "Sub/Secret"] {
            for result in [agent.read(path), agent.backlinks(path), agent.versions(path)] {
                assert!(result.unwrap_err().contains("not shared"));
            }
        }
        assert!(agent.search("zebra", 10).unwrap().as_array().unwrap().is_empty());
        assert!(!agent.health().unwrap().to_string().contains("Secret"));
    }

    #[test]
    fn a_shared_only_handle_changes_nothing_outside_the_ai_folder() {
        let (vault, data) = vault_with_notes();
        set_up_ai(&vault, &data);
        share(&vault, &data, "", true);
        let agent = open(&vault, &data, false).shared_only();
        let results = [
            agent.create("New", "x"),
            agent.append("Plan", "x", false),
            agent.edit("Plan", "oranges", "x", false),
            agent.set_properties("Plan", &Map::new(), &["a".into()]),
            agent.update_tags("Plan", &["a".into()], &[]),
            agent.rename("Plan", "AI/Plan", true),
            agent.delete("Plan"),
            agent.delete("Sub"),
            agent.create("AI/../New", "x"),
        ];
        for result in results {
            assert!(result.is_err());
        }
        assert!(agent.create("AI/Outputs/Draft", "draft").is_ok());
        assert!(agent.rename("AI/Outputs/Draft", "Draft", true).unwrap_err().contains("outside"));
        assert_eq!(std::fs::read_to_string(vault.path().join("Plan.md")).unwrap(), "The plan about oranges");
        assert!(!vault.path().join("New.md").exists() && !vault.path().join("Draft.md").exists());
    }

    #[test]
    fn instructions_and_hidden_notes_in_the_ai_folder_are_protected() {
        let (vault, data) = vault_with_notes();
        set_up_ai(&vault, &data);
        std::fs::write(vault.path().join("AI/Outputs/Mine.md"), "---\nai: false\n---\nkeep").unwrap();
        let agent = open(&vault, &data, false).shared_only();
        for result in [
            agent.edit("AI/AGENTS", "Instructions", "x", false),
            agent.append("AI/Preferences", "x", false),
            agent.create("AI/Instructions/Tone", "x"),
            agent.rename("AI/Context", "AI/Instructions/Context", true),
            agent.delete("AI/Workflows"),
            agent.delete("AI/Outputs"),
            agent.delete("AI"),
            agent.append("AI/Outputs/Mine", "x", false),
        ] {
            assert!(result.is_err());
        }
        assert!(agent.read("AI/Preferences").is_ok());
        AccessRules::update(data.path(), vault.path(), |rules| rules.edit_instructions = true).unwrap();
        assert!(agent.append("AI/Preferences", "- Brief", false).is_ok());
        assert!(agent.create("AI/Instructions/Tone", "x").is_ok());
        assert!(agent.delete("AI").unwrap_err().contains("itself"));
    }

    #[test]
    fn a_rename_leaves_links_outside_the_ai_folder_alone() {
        let (vault, data) = vault_with_notes();
        set_up_ai(&vault, &data);
        std::fs::write(vault.path().join("AI/Outputs/Draft.md"), "draft").unwrap();
        std::fs::write(vault.path().join("AI/Outputs/Index.md"), "[[Draft]]").unwrap();
        std::fs::write(vault.path().join("Home.md"), "[[Draft]]").unwrap();
        share(&vault, &data, "Home.md", true);
        let agent = open(&vault, &data, false).shared_only();
        let renamed = agent.rename("AI/Outputs/Draft", "AI/Outputs/Final", true).unwrap();
        assert_eq!((renamed["linksUpdated"].as_u64(), renamed["linksSkipped"].as_u64()), (Some(1), Some(1)));
        assert_eq!(std::fs::read_to_string(vault.path().join("Home.md")).unwrap(), "[[Draft]]");
        assert_eq!(std::fs::read_to_string(vault.path().join("AI/Outputs/Index.md")).unwrap(), "[[Final]]");
    }

    #[test]
    fn another_spelling_of_a_hidden_path_stays_hidden() {
        let (vault, data) = vault_with_notes();
        share(&vault, &data, "", true);
        share(&vault, &data, "Plan.md", false);
        let agent = open(&vault, &data, false).shared_only();
        assert!(agent.read("plan").is_err());
    }

    #[test]
    fn the_ai_folder_works_once_set_up_and_logs_go_there() {
        let (vault, data) = vault_with_notes();
        std::fs::create_dir(vault.path().join("AI")).unwrap();
        std::fs::write(vault.path().join("AI/Context.md"), "private until set up").unwrap();
        let agent = open(&vault, &data, false).shared_only();
        let before = agent.context().unwrap();
        assert_eq!((before["setUp"].as_bool(), before["content"].as_str()), (Some(false), Some("")));
        assert!(agent.set_context("x").unwrap_err().contains("not set up"));
        assert!(agent.append_to_daily("x").unwrap_err().contains("not set up"));

        set_up_ai(&vault, &data);
        agent.set_context("Launch").unwrap();
        agent.save_conversation("Kickoff", "Agreed").unwrap();
        let log = agent.append_to_daily("- Wrote the draft").unwrap();
        let path = log["path"].as_str().unwrap();
        assert!(path.starts_with("AI/Daily/"));
        assert!(std::fs::read_to_string(vault.path().join(path)).unwrap().ends_with("- Wrote the draft\n"));
        let context = agent.context().unwrap();
        assert_eq!(context["content"], "Launch\n");
        assert!(context["instructions"].as_str().unwrap().contains("AI assistants"));
        assert_eq!(context["recentConversations"].as_array().unwrap().len(), 1);
        assert!(!context["otherNotes"].to_string().contains("Daily"));

        std::fs::write(vault.path().join("AI/Context.md"), "---\nai: false\n---\nmine").unwrap();
        assert_eq!(agent.context().unwrap()["content"], "");
        assert!(agent.set_context("x").is_err());
    }

    #[test]
    fn a_users_daily_link_does_not_resolve_to_the_ai_log() {
        let paths = vec!["AI/Daily/2026-01-02.md".to_string(), "Journal/Daily notes/2026-01-02.md".to_string()];
        assert_eq!(
            crate::index::resolve_wiki_link_path("Home.md", "2026-01-02", &paths).as_deref(),
            Some("Journal/Daily notes/2026-01-02.md")
        );
    }

    #[test]
    fn the_selection_follows_renames() {
        let (vault, data) = vault_with_notes();
        share(&vault, &data, "Plan.md", true);
        open(&vault, &data, false).rename("Plan", "Sub/Plan", false).unwrap();
        let rules = AccessRules::load(data.path(), vault.path()).unwrap();
        assert!(rules.allows("Sub/Plan.md") && !rules.allows("Plan.md"));
    }

    #[test]
    fn reads_answer_from_the_files_on_disk() {
        let (vault, data) = vault_with_notes();
        let agent = open(&vault, &data, true);
        assert_eq!(agent.read("Plan").unwrap()["content"], "The plan about oranges");
        assert_eq!(agent.search("oranges", 10).unwrap()[0]["path"], "Plan.md");
        assert_eq!(agent.backlinks("Plan").unwrap()[0]["sourcePath"], "Home.md");
        assert_eq!(agent.tags(None).unwrap()[0]["tag"], "work");
        assert_eq!(agent.tasks().unwrap()[0]["dueDate"], "2026-11-01");
        assert_eq!(agent.list(None).unwrap().as_array().unwrap().len(), 2);
    }

    #[test]
    fn the_context_note_is_created_replaced_and_versioned() {
        let (vault, data) = vault_with_notes();
        let agent = open(&vault, &data, false);
        assert_eq!(agent.context().unwrap()["exists"], false);
        agent.set_context("Working on the Q4 plan").unwrap();
        agent.set_context("Working on the launch").unwrap();
        let context = agent.context().unwrap();
        assert_eq!(context["exists"], true);
        assert_eq!(context["content"], "Working on the launch\n");
        let versions = agent.versions("AI/Context").unwrap();
        assert_eq!(versions.as_array().unwrap().len(), 1);
    }

    #[test]
    fn a_read_only_handle_reads_context_but_cannot_set_it() {
        let (vault, data) = vault_with_notes();
        let agent = open(&vault, &data, true);
        assert!(agent.context().is_ok());
        assert!(agent.set_context("x").is_err());
    }

    #[test]
    fn a_read_only_handle_refuses_every_write() {
        let (vault, data) = vault_with_notes();
        let agent = open(&vault, &data, true);
        for result in [
            agent.create("New", "x"),
            agent.append("Plan", "x", false),
            agent.rename("Plan", "Plan2", true),
            agent.delete("Plan"),
        ] {
            assert!(result.unwrap_err().contains("read-only"));
        }
        assert_eq!(std::fs::read_to_string(vault.path().join("Plan.md")).unwrap(), "The plan about oranges");
        assert!(!vault.path().join("New.md").exists());
    }

    #[test]
    fn append_keeps_the_old_text_in_version_history() {
        let (vault, data) = vault_with_notes();
        let agent = open(&vault, &data, false);
        agent.append("Plan", "Second line", false).unwrap();
        assert_eq!(
            std::fs::read_to_string(vault.path().join("Plan.md")).unwrap(),
            "The plan about oranges\nSecond line\n"
        );
        let versions = agent.versions("Plan").unwrap();
        let id = versions[0]["id"].as_str().unwrap();
        assert_eq!(agent.version("Plan", id).unwrap()["content"], "The plan about oranges");
    }

    #[test]
    fn append_needs_create_missing_for_a_new_note() {
        let (vault, data) = vault_with_notes();
        let agent = open(&vault, &data, false);
        assert!(agent.append("Inbox", "idea", false).unwrap_err().contains("does not exist"));
        agent.append("Inbox", "idea", true).unwrap();
        assert_eq!(std::fs::read_to_string(vault.path().join("Inbox.md")).unwrap(), "idea\n");
    }

    #[test]
    fn rename_rewrites_links_and_snapshots_the_notes_it_changes() {
        let (vault, data) = vault_with_notes();
        let agent = open(&vault, &data, false);
        let out = agent.rename("Plan", "Sub/Roadmap", true).unwrap();
        assert_eq!(out["newPath"], "Sub/Roadmap.md");
        assert_eq!(out["notesUpdated"], 1);
        let home = std::fs::read_to_string(vault.path().join("Home.md")).unwrap();
        assert!(home.contains("[[Roadmap]]") || home.contains("[[Sub/Roadmap]]"), "{home}");
        let before = agent.versions("Home").unwrap();
        let id = before[0]["id"].as_str().unwrap();
        assert!(agent.version("Home", id).unwrap()["content"].as_str().unwrap().contains("[[Plan]]"));
    }

    #[test]
    fn paths_cannot_leave_the_vault() {
        let (vault, data) = vault_with_notes();
        let agent = open(&vault, &data, false);
        assert!(agent.read("../outside").is_err());
        assert!(agent.create("../outside", "x").is_err());
    }

    #[test]
    fn history_uses_the_apps_vault_identity_and_follows_renames() {
        let (vault, data) = vault_with_notes();
        let agent = open(&vault, &data, false);
        agent.append("Plan", "new text", false).unwrap();
        let app_versions = versions::versions_dir(data.path(), vault.path());
        assert_eq!(versions::list(&app_versions, "Plan.md").len(), 1);
        agent.rename("Plan", "Sub/Roadmap", true).unwrap();
        assert_eq!(agent.versions("Sub/Roadmap").unwrap().as_array().unwrap().len(), 1);
        assert!(versions::list(&app_versions, "Plan.md").is_empty());
    }

    #[test]
    fn folder_moves_rewrite_incoming_and_outgoing_links_and_move_history() {
        let (vault, data) = vault_with_notes();
        std::fs::write(vault.path().join("Sub/Inner.md"), "[[Sub/Inner]] [Plan](../Plan.md) ![image](image.png)").unwrap();
        std::fs::write(vault.path().join("Sub/image.png"), "image").unwrap();
        std::fs::write(vault.path().join("Home.md"), "[[Sub/Inner]] [Inner](Sub/Inner.md) ![image](Sub/image.png)").unwrap();
        let agent = open(&vault, &data, false);
        agent.append("Sub/Inner", "more", false).unwrap();
        agent.rename("Sub", "Nested/Moved", true).unwrap();
        let home = agent.read("Home").unwrap()["content"].as_str().unwrap().to_string();
        assert!(home.contains("[[Nested/Moved/Inner]]"), "{home}");
        assert!(home.contains("(Nested/Moved/Inner.md)"), "{home}");
        assert!(home.contains("(Nested/Moved/image.png)"), "{home}");
        let inner = agent.read("Nested/Moved/Inner").unwrap()["content"].as_str().unwrap().to_string();
        assert!(inner.contains("(../../Plan.md)"), "{inner}");
        assert!(inner.contains("(image.png)"), "{inner}");
        assert!(agent.versions("Nested/Moved/Inner").unwrap().as_array().unwrap().len() >= 2);
    }

    #[test]
    fn an_empty_list_folder_means_the_vault_root() {
        let (vault, data) = vault_with_notes();
        let agent = open(&vault, &data, true);
        assert_eq!(agent.list(Some("")).unwrap(), agent.list(None).unwrap());
        assert!(agent.list(Some("../outside")).is_err());
    }

    #[test]
    fn relative_vault_paths_use_the_same_history_as_absolute_paths() {
        let data = tempfile::tempdir().unwrap();
        let relative = std::path::Path::new(".");
        let absolute = std::path::absolute(relative).unwrap();
        let agent = AgentVault::open(relative, true, data.path()).unwrap();
        assert_eq!(agent.versions_dir, versions::versions_dir(data.path(), &absolute));
    }

    #[test]
    #[cfg(unix)]
    fn a_vault_root_alias_keeps_the_apps_history_identity() {
        let (vault, data) = vault_with_notes();
        let alias = data.path().join("Alias");
        std::os::unix::fs::symlink(vault.path(), &alias).unwrap();
        let agent = AgentVault::open(&alias, false, data.path()).unwrap();
        agent.append("Plan", "after", false).unwrap();
        assert_eq!(versions::list(&versions::versions_dir(data.path(), &alias), "Plan.md").len(), 1);
    }

    #[test]
    fn edit_replaces_one_exact_match_and_keeps_the_old_text() {
        let (vault, data) = vault_with_notes();
        let agent = open(&vault, &data, false);
        agent.edit("Home", "- [ ] Pay", "- [x] Pay", false).unwrap();
        assert!(agent.read("Home").unwrap()["content"].as_str().unwrap().contains("- [x] Pay"));
        assert_eq!(agent.versions("Home").unwrap().as_array().unwrap().len(), 1);
        assert!(agent.edit("Home", "not there", "x", false).unwrap_err().contains("does not appear"));

        std::fs::write(vault.path().join("Twice.md"), "a\r\nb\r\na\r\nb\r\n").unwrap();
        assert!(agent.edit("Twice", "a\nb", "c", false).unwrap_err().contains("2 times"));
        assert_eq!(agent.edit("Twice", "a\nb", "c", true).unwrap()["replacements"], 2);
        assert_eq!(std::fs::read_to_string(vault.path().join("Twice.md")).unwrap(), "c\r\nc\r\n");
    }

    #[test]
    fn edits_are_refused_on_a_read_only_handle() {
        let (vault, data) = vault_with_notes();
        let agent = open(&vault, &data, true);
        assert!(agent.edit("Plan", "plan", "x", false).is_err());
        assert!(agent.update_tags("Plan", &["x".into()], &[]).is_err());
        assert!(agent.save_conversation("Chat", "text").is_err());
        assert!(agent.append_to_daily("text").is_err());
    }

    #[test]
    fn properties_and_tags_change_only_the_frontmatter() {
        let (vault, data) = vault_with_notes();
        let agent = open(&vault, &data, false);
        std::fs::write(vault.path().join("Tagged.md"), "---\ntags:\n  - work\n  - old\n---\nBody #old\n").unwrap();
        let answer = agent.update_tags("Tagged", &["#new".into()], &["old".into()]).unwrap();
        assert_eq!(answer["tags"], json!(["work", "new"]));
        assert_eq!(answer["stillInBody"], json!(["old"]));
        let mut set = Map::new();
        set.insert("status".into(), json!("done"));
        agent.set_properties("Tagged", &set, &[]).unwrap();
        assert_eq!(
            std::fs::read_to_string(vault.path().join("Tagged.md")).unwrap(),
            "---\ntags: [\"work\",\"new\"]\nstatus: \"done\"\n---\nBody #old\n"
        );
        assert!(agent.update_tags("Tagged", &["bad tag".into()], &[]).is_err());
    }

    #[test]
    fn tag_updates_fold_case_variants_and_quoted_aliases_without_losing_other_tags() {
        let (vault, data) = vault_with_notes();
        let agent = open(&vault, &data, false);
        for key in ["Tags", "Tag", "TAG", "\"Tags\"", "'tag'"] {
            std::fs::write(vault.path().join("Tagged.md"),
                format!("---\n{key}: [old, keep]\nstatus: draft\n---\nBody #old\n")).unwrap();
            let result = agent.update_tags("Tagged", &["new".into()], &["old".into()]).unwrap();
            assert_eq!(result["tags"], json!(["keep", "new"]));
            assert_eq!(result["stillInBody"], json!(["old"]));
            let text = std::fs::read_to_string(vault.path().join("Tagged.md")).unwrap();
            assert_eq!(text, "---\nstatus: draft\ntags: [\"keep\",\"new\"]\n---\nBody #old\n");
            agent.update_tags("Tagged", &[], &["keep".into(), "new".into()]).unwrap();
            assert_eq!(std::fs::read_to_string(vault.path().join("Tagged.md")).unwrap(),
                "---\nstatus: draft\n---\nBody #old\n");
        }
        std::fs::write(vault.path().join("Tagged.md"), "---\nTags: [old]\n---\nBody\n").unwrap();
        let result = agent.update_tags("Tagged", &[], &["old".into()]).unwrap();
        assert_eq!(result["stillInBody"], json!([]));
        assert_eq!(std::fs::read_to_string(vault.path().join("Tagged.md")).unwrap(), "Body\n");
    }

    #[test]
    fn one_conversation_keeps_one_note_and_the_context_lists_it() {
        let (vault, data) = vault_with_notes();
        let agent = open(&vault, &data, false);
        let first = agent.save_conversation("Launch: plan / budget?", "Decided the date.").unwrap();
        assert_eq!(first["created"], true);
        let path = first["path"].as_str().unwrap().to_string();
        assert!(path.starts_with("AI/Conversations/") && path.ends_with(" Launch plan budget.md"), "{path}");
        let second = agent.save_conversation("launch plan budget", "Moved it a week.").unwrap();
        assert_eq!(second["path"], path.as_str());
        let text = agent.read(&path).unwrap()["content"].as_str().unwrap().to_string();
        assert!(text.starts_with("# Launch plan budget\n\nDecided the date.\n") && text.ends_with("Moved it a week.\n"), "{text}");

        agent.set_context("Now").unwrap();
        let context = agent.context().unwrap();
        assert_eq!(context["recentConversations"][0]["path"], path.as_str());
        assert_eq!(context["otherNotes"], json!([]));
        assert_eq!(conversation_title(" .. "), "Conversation");
    }

    #[test]
    fn the_daily_note_follows_the_apps_folder_and_template() {
        let (vault, data) = vault_with_notes();
        std::fs::write(data.path().join("settings.json"), r#"{"jotter.dailyFolder":"Journal","jotter.dailyTemplatePath":"Tpl"}"#).unwrap();
        std::fs::write(vault.path().join("Tpl.md"), "# {{title}}\n").unwrap();
        let agent = open(&vault, &data, false);
        let first = agent.append_to_daily("- 09:00 started").unwrap();
        assert_eq!(first["created"], true);
        let date = chrono::Local::now().format("%Y-%m-%d").to_string();
        assert_eq!(first["path"], format!("Journal/{date}.md"));
        agent.append_to_daily("- 10:00 more").unwrap();
        assert_eq!(
            agent.read(&format!("Journal/{date}")).unwrap()["content"],
            format!("# {date}\n- 09:00 started\n- 10:00 more\n")
        );
    }

    #[test]
    fn cached_search_detects_equal_size_changes_with_preserved_mtime() {
        let (vault, data) = vault_with_notes();
        let path = vault.path().join("Fruit.md");
        std::fs::write(&path, "bananas").unwrap();
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        let agent = open(&vault, &data, false);
        assert_eq!(agent.search("bananas", 10).unwrap()[0]["path"], "Fruit.md");
        std::fs::write(&path, "oranges").unwrap();
        std::fs::File::options().write(true).open(&path).unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(modified)).unwrap();
        assert_eq!(agent.search("oranges", 10).unwrap()[0]["path"], "Fruit.md");
    }

    #[test]
    fn another_process_must_wait_for_a_vault_writer() {
        const CHILD_ROOT: &str = "JOTTER_TEST_WRITE_LOCK_ROOT";
        const CHILD_DATA: &str = "JOTTER_TEST_WRITE_LOCK_DATA";
        const CHILD_EXPECT_BUSY: &str = "JOTTER_TEST_WRITE_LOCK_BUSY";

        if let Some(root) = std::env::var_os(CHILD_ROOT) {
            let data = PathBuf::from(std::env::var_os(CHILD_DATA).unwrap());
            let agent = AgentVault::open(Path::new(&root), false, &data).unwrap();
            let result = agent.append("Note.md", "child", false);
            if std::env::var_os(CHILD_EXPECT_BUSY).is_some() {
                assert!(result.unwrap_err().contains("Another Jotter process"));
            } else {
                result.unwrap();
            }
            return;
        }

        let root = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let note = root.path().join("Note.md");
        std::fs::write(&note, "original\n").unwrap();
        let run_child = |busy: bool| {
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", "agent::tests::another_process_must_wait_for_a_vault_writer"])
                .env(CHILD_ROOT, root.path().join("."))
                .env(CHILD_DATA, data.path());
            if busy {
                command.env(CHILD_EXPECT_BUSY, "1");
            }
            let output = command.output().unwrap();
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stdout));
        };
        let lock = crate::vault_lock::VaultWriteLock::acquire(data.path(), root.path()).unwrap();
        run_child(true);
        assert_eq!(std::fs::read_to_string(&note).unwrap(), "original\n");
        drop(lock);
        run_child(false);
        assert_eq!(std::fs::read_to_string(&note).unwrap(), "original\nchild\n");
    }

    #[test]
    fn a_reused_search_index_sees_changed_files() {
        let (vault, data) = vault_with_notes();
        let agent = open(&vault, &data, false);
        assert_eq!(agent.search("bananas", 10).unwrap().as_array().unwrap().len(), 0);
        std::fs::write(vault.path().join("Fruit.md"), "bananas").unwrap();
        assert_eq!(agent.search("bananas", 10).unwrap()[0]["path"], "Fruit.md");
        assert_eq!(agent.list(None).unwrap().as_array().unwrap().len(), 3);
    }
}
