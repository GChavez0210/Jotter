use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};

use crate::clipboard::strip_markdown_to_plain;
use crate::links::{extract_tags, parse_wiki_links_with_lines, split_yaml_frontmatter};
use crate::types::{
    BacklinkEntry, CalendarDateField, CalendarDaySummary, CalendarNoteEntry, IndexedLink,
    TagSummary,
};

/// Filesystem metadata and parsed links/tags scanned from a `.md` note without holding the SQLite mutex.
#[derive(Debug, Clone)]
pub struct ScannedNoteFile {
    pub path: String,
    pub title: String,
    pub fs_modified_ms: i64,
    pub created_ms: i64,
    pub preview: String,
    pub content: String,
    pub links: Vec<IndexedLink>,
    pub tags: Vec<String>,
}

/// Normalizes a wiki-link target for case-insensitive matching (strips `.md` suffix, normalizes slashes).
pub fn normalize_link_target(raw: &str) -> String {
    let slash_norm = raw.trim().replace('\\', "/");
    let stripped = slash_norm
        .strip_suffix(".md")
        .or_else(|| slash_norm.strip_suffix(".MD"))
        .unwrap_or(&slash_norm);
    stripped.trim().to_lowercase()
}

/// Converts a `SystemTime` to milliseconds since UNIX_EPOCH.
pub fn system_time_to_ms(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Converts epoch milliseconds to an ISO `YYYY-MM-DD` date string (Howard Hinnant civil-from-days).
pub fn ms_to_ymd_date(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };
    format!("{year:04}-{m:02}-{d:02}")
}

/// Extracts a concise single-line preview from a Markdown note (skipping YAML frontmatter and blank lines).
pub fn extract_note_preview(content: &str) -> String {
    let (body, _) = split_yaml_frontmatter(content);
    let mut in_fence = false;

    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        let plain = strip_markdown_to_plain(trimmed);
        let clean = plain.trim();
        if !clean.is_empty() {
            let truncated: String = clean.chars().take(120).collect();
            return truncated;
        }
    }

    String::new()
}

/// Initializes the rebuildable SQLite metadata cache schema (`notes`, `links`, `note_tags`, and persistent `edit_log`).
pub fn init_index_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS notes (
            path TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            modified_ms INTEGER NOT NULL,
            modified_date TEXT NOT NULL,
            created_ms INTEGER NOT NULL,
            created_date TEXT NOT NULL,
            preview TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS edit_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            path TEXT NOT NULL,
            edited_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS links (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source_path TEXT NOT NULL,
            target TEXT NOT NULL,
            target_norm TEXT NOT NULL,
            resolved_path TEXT,
            heading TEXT,
            alias TEXT,
            line_number INTEGER NOT NULL,
            line_context TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS note_tags (
            path TEXT NOT NULL,
            tag TEXT NOT NULL,
            PRIMARY KEY (path, tag)
        );",
    )
    .map_err(|e| format!("Failed to initialize SQLite index schema: {e}"))?;

    // Ensure `resolved_path` exists if upgrading an older schema on disk
    let _ = conn.execute("ALTER TABLE links ADD COLUMN resolved_path TEXT", []);

    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_notes_modified_date ON notes(modified_date);
        CREATE INDEX IF NOT EXISTS idx_notes_created_date ON notes(created_date);
        CREATE INDEX IF NOT EXISTS idx_edit_log_path_ms ON edit_log(path, edited_ms DESC);
        CREATE INDEX IF NOT EXISTS idx_links_source_path ON links(source_path);
        CREATE INDEX IF NOT EXISTS idx_links_target_norm ON links(target_norm);
        CREATE INDEX IF NOT EXISTS idx_links_resolved_path ON links(resolved_path);
        CREATE INDEX IF NOT EXISTS idx_note_tags_tag ON note_tags(tag);",
    )
    .map_err(|e| format!("Failed to initialize SQLite index indices: {e}"))
}

/// Lookup table that resolves wiki-link targets to a single vault note path using:
/// 1. Exact relative path first
/// 2. Same folder as the source note
/// 3. Shortest path among notes with that stem (tie-broken lexicographically)
pub struct WikiLinkResolver {
    exact_map: std::collections::HashMap<String, String>,
    stem_map: std::collections::HashMap<String, String>,
}

impl WikiLinkResolver {
    pub fn new(all_note_paths: &[String]) -> Self {
        let mut exact_map = std::collections::HashMap::with_capacity(all_note_paths.len());
        let mut stem_map: std::collections::HashMap<String, String> =
            std::collections::HashMap::with_capacity(all_note_paths.len());

        for path in all_note_paths {
            let norm_rel = normalize_link_target(path);
            if norm_rel.is_empty() {
                continue;
            }
            exact_map.insert(norm_rel, path.clone());

            let stem = path
                .rsplit('/')
                .next()
                .map(normalize_link_target)
                .unwrap_or_default();
            if !stem.is_empty() {
                match stem_map.get(&stem) {
                    Some(existing)
                        if (existing.len(), existing.as_str()) <= (path.len(), path.as_str()) => {}
                    _ => {
                        stem_map.insert(stem, path.clone());
                    }
                }
            }
        }

        Self {
            exact_map,
            stem_map,
        }
    }

    pub fn resolve(&self, source_path: &str, raw_target: &str) -> Option<String> {
        let target_norm = normalize_link_target(raw_target);
        if target_norm.is_empty() {
            return None;
        }

        // 1. Exact relative path first
        if let Some(exact) = self.exact_map.get(&target_norm) {
            return Some(exact.clone());
        }

        // 2. Same folder as the source note
        let source_norm = source_path.trim().replace('\\', "/");
        if let Some((source_dir, _)) = source_norm.rsplit_once('/') {
            if !source_dir.is_empty() {
                let same_folder_norm = normalize_link_target(&format!("{source_dir}/{target_norm}"));
                if let Some(same_folder_match) = self.exact_map.get(&same_folder_norm) {
                    return Some(same_folder_match.clone());
                }
            }
        }

        // 3. Shortest path among notes with that stem
        if !target_norm.contains('/') {
            if let Some(shortest) = self.stem_map.get(&target_norm) {
                return Some(shortest.clone());
            }
        }

        None
    }
}

/// Resolves a single wiki link against `all_note_paths`.
pub fn resolve_wiki_link_path(
    source_path: &str,
    raw_target: &str,
    all_note_paths: &[String],
) -> Option<String> {
    WikiLinkResolver::new(all_note_paths).resolve(source_path, raw_target)
}

/// Re-resolves all links in the `links` table against the current set of notes in `notes`.
pub fn re_resolve_all_links(conn: &Connection) -> Result<(), String> {
    let mut notes_stmt = conn
        .prepare("SELECT path FROM notes ORDER BY path ASC")
        .map_err(|e| format!("Failed to prepare notes query for link resolution: {e}"))?;
    let all_note_paths: Vec<String> = notes_stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| format!("Failed to read note paths for link resolution: {e}"))?
        .filter_map(Result::ok)
        .collect();

    let resolver = WikiLinkResolver::new(&all_note_paths);

    let mut links_stmt = conn
        .prepare("SELECT id, source_path, target, resolved_path FROM links")
        .map_err(|e| format!("Failed to prepare links query for resolution: {e}"))?;
    let link_rows: Vec<(i64, String, String, Option<String>)> = links_stmt
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .map_err(|e| format!("Failed to read links for resolution: {e}"))?
        .filter_map(Result::ok)
        .collect();

    let mut update_stmt = conn
        .prepare("UPDATE links SET resolved_path = ?1 WHERE id = ?2")
        .map_err(|e| format!("Failed to prepare resolved_path update: {e}"))?;

    for (id, source_path, target, current_resolved) in link_rows {
        let new_resolved = resolver.resolve(&source_path, &target);
        if new_resolved != current_resolved {
            update_stmt
                .execute(params![new_resolved, id])
                .map_err(|e| format!("Failed to update resolved_path for link {id}: {e}"))?;
        }
    }

    Ok(())
}

/// Logs an in-app save event in `edit_log` (`path`, `edited_ms`), which is preserved across index rebuilds.
pub fn record_edit_log(conn: &Connection, rel_path: &str, edited_ms: i64) -> Result<(), String> {
    conn.execute(
        "INSERT INTO edit_log (path, edited_ms) VALUES (?1, ?2)",
        params![rel_path, edited_ms],
    )
    .map_err(|e| format!("Failed to insert into edit_log for {rel_path}: {e}"))?;
    Ok(())
}

/// Returns the latest `edited_ms` from `edit_log` for `rel_path`, if any in-app save has been recorded.
pub fn latest_edit_log_ms(conn: &Connection, rel_path: &str) -> Result<Option<i64>, String> {
    conn.query_row(
        "SELECT MAX(edited_ms) FROM edit_log WHERE path = ?1",
        params![rel_path],
        |row| row.get::<_, Option<i64>>(0),
    )
    .optional()
    .map(|opt| opt.flatten())
    .map_err(|e| format!("Failed to query edit_log for {rel_path}: {e}"))
}

/// Updates `edit_log` paths when a note or folder is renamed so in-app edit timestamps follow the file.
pub fn rename_edit_log_paths(
    conn: &Connection,
    old_rel_path: &str,
    new_rel_path: &str,
) -> Result<(), String> {
    conn.execute(
        "UPDATE edit_log SET path = ?2 WHERE path = ?1",
        params![old_rel_path, new_rel_path],
    )
    .map_err(|e| format!("Failed to rename edit_log path {old_rel_path}: {e}"))?;

    let old_prefix = format!("{old_rel_path}/%");
    let old_prefix_len = old_rel_path.len() + 1;
    let mut stmt = conn
        .prepare("SELECT DISTINCT path FROM edit_log WHERE path LIKE ?1")
        .map_err(|e| format!("Failed to prepare edit_log prefix query: {e}"))?;
    let sub_paths: Vec<String> = stmt
        .query_map(params![old_prefix], |r| r.get(0))
        .map_err(|e| format!("Failed to query edit_log sub-paths: {e}"))?
        .filter_map(Result::ok)
        .collect();

    for sub in sub_paths {
        if sub.len() >= old_prefix_len {
            let updated = format!("{new_rel_path}/{}", &sub[old_prefix_len..]);
            conn.execute(
                "UPDATE edit_log SET path = ?2 WHERE path = ?1",
                params![sub, updated],
            )
            .map_err(|e| format!("Failed to update child edit_log path {sub}: {e}"))?;
        }
    }

    Ok(())
}

/// Inserts or updates a note's metadata row in the SQLite index cache.
pub fn upsert_note_entry(
    conn: &Connection,
    entry: &CalendarNoteEntry,
    modified_date: &str,
    created_date: &str,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO notes (path, title, modified_ms, modified_date, created_ms, created_date, preview)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(path) DO UPDATE SET
            title = excluded.title,
            modified_ms = excluded.modified_ms,
            modified_date = excluded.modified_date,
            created_ms = excluded.created_ms,
            created_date = excluded.created_date,
            preview = excluded.preview",
        params![
            entry.path,
            entry.title,
            entry.modified_ms,
            modified_date,
            entry.created_ms,
            created_date,
            entry.preview
        ],
    )
    .map_err(|e| format!("Failed to upsert note index entry: {e}"))?;
    Ok(())
}

/// Replaces all outgoing `links` rows for `source_path`.
pub fn replace_note_links(
    conn: &Connection,
    source_path: &str,
    links: &[IndexedLink],
) -> Result<(), String> {
    conn.execute(
        "DELETE FROM links WHERE source_path = ?1",
        params![source_path],
    )
    .map_err(|e| format!("Failed to clear links for {source_path}: {e}"))?;

    for link in links {
        let target_norm = normalize_link_target(&link.target);
        if target_norm.is_empty() {
            continue;
        }
        conn.execute(
            "INSERT INTO links (source_path, target, target_norm, resolved_path, heading, alias, line_number, line_context)
             VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?6, ?7)",
            params![
                source_path,
                link.target,
                target_norm,
                link.heading,
                link.alias,
                link.line_number as i64,
                link.line_context
            ],
        )
        .map_err(|e| format!("Failed to insert link for {source_path}: {e}"))?;
    }

    Ok(())
}

/// Replaces all `note_tags` rows for `path`.
pub fn replace_note_tags(
    conn: &Connection,
    path: &str,
    tags: &[String],
) -> Result<(), String> {
    conn.execute("DELETE FROM note_tags WHERE path = ?1", params![path])
        .map_err(|e| format!("Failed to clear tags for {path}: {e}"))?;

    for tag in tags {
        conn.execute(
            "INSERT OR IGNORE INTO note_tags (path, tag) VALUES (?1, ?2)",
            params![path, tag],
        )
        .map_err(|e| format!("Failed to insert tag {tag} for {path}: {e}"))?;
    }

    Ok(())
}

/// Scans a single `.md` file from disk without holding the SQLite lock.
pub fn scan_single_file_from_disk(
    vault_root: &Path,
    abs_path: &Path,
) -> Result<ScannedNoteFile, String> {
    let rel = abs_path
        .strip_prefix(vault_root)
        .map_err(|e| format!("Path is outside vault root: {e}"))?;
    let rel_str = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");

    let metadata = fs::metadata(abs_path)
        .map_err(|e| format!("Failed to read metadata for {}: {e}", abs_path.display()))?;
    let fs_modified_ms = metadata
        .modified()
        .map(system_time_to_ms)
        .unwrap_or(0);
    let created_ms = metadata
        .created()
        .map(system_time_to_ms)
        .unwrap_or(fs_modified_ms);

    let content = fs::read_to_string(abs_path).unwrap_or_default();
    let title = abs_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string();
    let preview = extract_note_preview(&content);
    let links = parse_wiki_links_with_lines(&rel_str, &content);
    let tags = extract_tags(&content);

    Ok(ScannedNoteFile {
        path: rel_str,
        title,
        fs_modified_ms,
        created_ms,
        preview,
        content,
        links,
        tags,
    })
}

/// Recursively scans all `.md` files under `dir` (inside `vault_root`) without holding the SQLite lock.
/// Ignores `.obsidian` and dot-entries.
pub fn scan_dir_files_from_disk(
    vault_root: &Path,
    dir: &Path,
) -> Result<Vec<ScannedNoteFile>, String> {
    let mut out = Vec::new();
    walk_and_scan_dir(vault_root, dir, &mut out)?;
    Ok(out)
}

fn walk_and_scan_dir(
    vault_root: &Path,
    current_dir: &Path,
    out: &mut Vec<ScannedNoteFile>,
) -> Result<(), String> {
    let entries = fs::read_dir(current_dir)
        .map_err(|e| format!("Failed to read directory {}: {e}", current_dir.display()))?;

    for entry_res in entries {
        let entry = match entry_res {
            Ok(e) => e,
            Err(_) => continue,
        };
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        // Ignore .obsidian, .git, and hidden/temp dot-files
        if name_str.starts_with('.') {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            walk_and_scan_dir(vault_root, &path, out)?;
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        {
            if let Ok(scanned) = scan_single_file_from_disk(vault_root, &path) {
                out.push(scanned);
            }
        }
    }

    Ok(())
}

fn upsert_scanned_note_no_resolve(
    conn: &Connection,
    scanned: &ScannedNoteFile,
    in_app_save_ms: Option<i64>,
) -> Result<(), String> {
    if let Some(save_ms) = in_app_save_ms {
        record_edit_log(conn, &scanned.path, save_ms)?;
    }

    let effective_mod_ms = in_app_save_ms
        .or_else(|| latest_edit_log_ms(conn, &scanned.path).ok().flatten())
        .unwrap_or(scanned.fs_modified_ms);

    let entry = CalendarNoteEntry {
        path: scanned.path.clone(),
        title: scanned.title.clone(),
        modified_ms: effective_mod_ms,
        created_ms: scanned.created_ms,
        preview: scanned.preview.clone(),
    };

    upsert_note_entry(
        conn,
        &entry,
        &ms_to_ymd_date(effective_mod_ms),
        &ms_to_ymd_date(scanned.created_ms),
    )?;
    replace_note_links(conn, &scanned.path, &scanned.links)?;
    replace_note_tags(conn, &scanned.path, &scanned.tags)?;
    Ok(())
}

/// Upserts a pre-scanned `.md` file into SQLite (`notes`, `links`, and `note_tags`),
/// consulting `edit_log` for the latest in-app `edited_ms` and falling back to `scanned.fs_modified_ms`,
/// then re-resolves all affected wiki links across the vault.
pub fn upsert_scanned_note(
    conn: &Connection,
    scanned: &ScannedNoteFile,
    in_app_save_ms: Option<i64>,
) -> Result<(), String> {
    upsert_scanned_note_no_resolve(conn, scanned, in_app_save_ms)?;
    re_resolve_all_links(conn)
}

/// Indexes a single `.md` file from disk into the SQLite cache.
/// If `override_modified_ms` is `Some(ms)`, logs `(path, ms)` in `edit_log`.
/// Otherwise uses the latest `edit_log` entry when present and falls back to file `mtime`.
pub fn index_single_file(
    conn: &Connection,
    vault_root: &Path,
    abs_path: &Path,
    override_modified_ms: Option<i64>,
) -> Result<(), String> {
    let scanned = scan_single_file_from_disk(vault_root, abs_path)?;
    upsert_scanned_note(conn, &scanned, override_modified_ms)
}

/// Removes a note or all notes inside a folder (`rel_path` or `rel_path/%`) from `notes`, `links`, and `note_tags`,
/// and re-resolves affected links across the vault.
pub fn delete_indexed_path(conn: &Connection, rel_path: &str) -> Result<(), String> {
    let prefix = format!("{rel_path}/%");
    conn.execute(
        "DELETE FROM notes WHERE path = ?1 OR path LIKE ?2",
        params![rel_path, prefix],
    )
    .map_err(|e| format!("Failed to delete indexed path {rel_path} from notes: {e}"))?;
    conn.execute(
        "DELETE FROM links WHERE source_path = ?1 OR source_path LIKE ?2",
        params![rel_path, prefix],
    )
    .map_err(|e| format!("Failed to delete indexed path {rel_path} from links: {e}"))?;
    conn.execute(
        "DELETE FROM note_tags WHERE path = ?1 OR path LIKE ?2",
        params![rel_path, prefix],
    )
    .map_err(|e| format!("Failed to delete indexed path {rel_path} from note_tags: {e}"))?;
    re_resolve_all_links(conn)
}

/// Applies a pre-scanned list of vault files to rebuild `notes`, `links`, and `note_tags` while preserving `edit_log`.
pub fn apply_scanned_vault_index(
    conn: &Connection,
    scanned_files: &[ScannedNoteFile],
) -> Result<usize, String> {
    init_index_schema(conn)?;
    conn.execute_batch(
        "DELETE FROM notes;
         DELETE FROM links;
         DELETE FROM note_tags;",
    )
    .map_err(|e| format!("Failed to clear derived index tables: {e}"))?;

    for scanned in scanned_files {
        upsert_scanned_note_no_resolve(conn, scanned, None)?;
    }

    re_resolve_all_links(conn)?;
    Ok(scanned_files.len())
}

/// Rebuilds the SQLite `notes`, `links`, and `note_tags` tables from `.md` files in `vault_root`
/// (ignoring `.obsidian` and dot-entries), while preserving all rows in `edit_log`.
pub fn rebuild_index_from_vault(conn: &Connection, vault_root: &Path) -> Result<usize, String> {
    let scanned = scan_dir_files_from_disk(vault_root, vault_root)?;
    apply_scanned_vault_index(conn, &scanned)
}

/// Queries all backlinks pointing to `active_rel_path` using the index-time `resolved_path` column.
pub fn query_backlinks_for_note(
    conn: &Connection,
    active_rel_path: &str,
) -> Result<Vec<BacklinkEntry>, String> {
    let normalized_path = active_rel_path.trim().replace('\\', "/");

    let mut stmt = conn
        .prepare(
            "SELECT l.source_path, COALESCE(n.title, l.source_path), l.target, l.heading, l.alias, l.line_number, l.line_context
             FROM links l
             LEFT JOIN notes n ON n.path = l.source_path
             WHERE l.resolved_path = ?1
               AND l.source_path != ?1
             ORDER BY l.source_path ASC, l.line_number ASC",
        )
        .map_err(|e| format!("Failed to prepare backlinks query: {e}"))?;

    let rows = stmt
        .query_map(params![normalized_path], |row| {
            Ok(BacklinkEntry {
                source_path: row.get(0)?,
                source_title: row.get(1)?,
                target: row.get(2)?,
                heading: row.get(3)?,
                alias: row.get(4)?,
                line_number: row.get::<_, i64>(5)? as usize,
                line_context: row.get(6)?,
            })
        })
        .map_err(|e| format!("Failed to execute backlinks query: {e}"))?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("Failed to read backlink row: {e}"))?);
    }
    Ok(out)
}

/// Queries all tags across the vault with their note counts, sorted by count descending then tag name.
pub fn query_all_tags(conn: &Connection) -> Result<Vec<TagSummary>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT tag, COUNT(*) AS cnt
             FROM note_tags
             GROUP BY tag
             ORDER BY cnt DESC, tag ASC",
        )
        .map_err(|e| format!("Failed to prepare tags query: {e}"))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(TagSummary {
                tag: row.get(0)?,
                count: row.get::<_, usize>(1)?,
            })
        })
        .map_err(|e| format!("Failed to execute tags query: {e}"))?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("Failed to read tag summary row: {e}"))?);
    }
    Ok(out)
}

/// Queries all notes containing a given `tag`, sorted by `modified_ms` descending.
pub fn query_notes_by_tag(
    conn: &Connection,
    tag: &str,
) -> Result<Vec<CalendarNoteEntry>, String> {
    let clean_tag = tag.trim().trim_start_matches('#');
    let mut stmt = conn
        .prepare(
            "SELECT n.path, n.title, n.modified_ms, n.created_ms, n.preview
             FROM notes n
             INNER JOIN note_tags t ON n.path = t.path
             WHERE LOWER(t.tag) = LOWER(?1)
             ORDER BY n.modified_ms DESC",
        )
        .map_err(|e| format!("Failed to prepare notes-by-tag query: {e}"))?;

    let rows = stmt
        .query_map(params![clean_tag], |row| {
            Ok(CalendarNoteEntry {
                path: row.get(0)?,
                title: row.get(1)?,
                modified_ms: row.get(2)?,
                created_ms: row.get(3)?,
                preview: row.get(4)?,
            })
        })
        .map_err(|e| format!("Failed to execute notes-by-tag query: {e}"))?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("Failed to read tagged note row: {e}"))?);
    }
    Ok(out)
}

/// Queries per-day note counts for a given `year_month` prefix (e.g. `"2026-10"`).
/// For `CalendarDateField::Modified`, uses the latest `edit_log` entry when present and falls back to file `mtime`.
pub fn query_calendar_month(
    conn: &Connection,
    year_month: &str,
    field: CalendarDateField,
) -> Result<Vec<CalendarDaySummary>, String> {
    let prefix = format!("{year_month}-%");
    let sql = match field {
        CalendarDateField::Modified => {
            "SELECT modified_date, COUNT(*) FROM notes WHERE modified_date LIKE ?1 GROUP BY modified_date ORDER BY modified_date ASC"
        }
        CalendarDateField::Created => {
            "SELECT created_date, COUNT(*) FROM notes WHERE created_date LIKE ?1 GROUP BY created_date ORDER BY created_date ASC"
        }
    };

    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| format!("Failed to prepare month query: {e}"))?;
    let rows = stmt
        .query_map(params![prefix], |row| {
            Ok(CalendarDaySummary {
                date: row.get(0)?,
                count: row.get::<_, usize>(1)?,
            })
        })
        .map_err(|e| format!("Failed to execute month query: {e}"))?;

    let mut summaries = Vec::new();
    for row in rows {
        summaries.push(row.map_err(|e| format!("Failed to read month summary row: {e}"))?);
    }
    Ok(summaries)
}

/// Queries notes touched on a specific `YYYY-MM-DD` date, sorted most-recent first.
pub fn query_notes_on_date(
    conn: &Connection,
    date: &str,
    field: CalendarDateField,
) -> Result<Vec<CalendarNoteEntry>, String> {
    let sql = match field {
        CalendarDateField::Modified => {
            "SELECT path, title, modified_ms, created_ms, preview FROM notes WHERE modified_date = ?1 ORDER BY modified_ms DESC"
        }
        CalendarDateField::Created => {
            "SELECT path, title, modified_ms, created_ms, preview FROM notes WHERE created_date = ?1 ORDER BY created_ms DESC"
        }
    };

    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| format!("Failed to prepare day notes query: {e}"))?;
    let rows = stmt
        .query_map(params![date], |row| {
            Ok(CalendarNoteEntry {
                path: row.get(0)?,
                title: row.get(1)?,
                modified_ms: row.get(2)?,
                created_ms: row.get(3)?,
                preview: row.get(4)?,
            })
        })
        .map_err(|e| format!("Failed to execute day notes query: {e}"))?;

    let mut entries = Vec::new();
    for row in rows {
        entries.push(row.map_err(|e| format!("Failed to read day note row: {e}"))?);
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_date_queries_aggregate_and_sort_by_modified_and_created() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        init_index_schema(&conn).expect("schema");

        let note1 = CalendarNoteEntry {
            path: "daily/2026-10-18.md".to_string(),
            title: "2026-10-18".to_string(),
            modified_ms: 1792360000000,
            created_ms: 1792270000000,
            preview: "Morning standup notes".to_string(),
        };
        let note2 = CalendarNoteEntry {
            path: "projects/jotter.md".to_string(),
            title: "Jotter".to_string(),
            modified_ms: 1792365000000,
            created_ms: 1792365000000,
            preview: "Clipboard spike implementation".to_string(),
        };

        upsert_note_entry(&conn, &note1, "2026-10-18", "2026-10-17").unwrap();
        upsert_note_entry(&conn, &note2, "2026-10-18", "2026-10-18").unwrap();

        let mod_days =
            query_calendar_month(&conn, "2026-10", CalendarDateField::Modified).unwrap();
        assert_eq!(
            mod_days,
            vec![CalendarDaySummary {
                date: "2026-10-18".to_string(),
                count: 2
            }]
        );

        let created_days =
            query_calendar_month(&conn, "2026-10", CalendarDateField::Created).unwrap();
        assert_eq!(
            created_days,
            vec![
                CalendarDaySummary {
                    date: "2026-10-17".to_string(),
                    count: 1
                },
                CalendarDaySummary {
                    date: "2026-10-18".to_string(),
                    count: 1
                }
            ]
        );

        let notes_on_18 =
            query_notes_on_date(&conn, "2026-10-18", CalendarDateField::Modified).unwrap();
        assert_eq!(notes_on_18.len(), 2);
        // Most recently modified note comes first
        assert_eq!(notes_on_18[0].title, "Jotter");
        assert_eq!(notes_on_18[1].title, "2026-10-18");
    }

    #[test]
    fn rebuild_index_skips_obsidian_folder_and_indexes_markdown_notes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();

        fs::create_dir_all(root.join(".obsidian")).unwrap();
        fs::write(root.join(".obsidian/workspace.json"), "{}").unwrap();
        fs::write(root.join(".obsidian/sneaky.md"), "# Ignored").unwrap();

        fs::create_dir_all(root.join("Folder")).unwrap();
        fs::write(
            root.join("Welcome.md"),
            "---\ntags: [intro]\n---\n# Welcome Note\nBody line",
        )
        .unwrap();
        fs::write(root.join("Folder/SubNote.md"), "Second **note** preview").unwrap();

        let conn = Connection::open_in_memory().unwrap();
        let indexed_count = rebuild_index_from_vault(&conn, root).unwrap();
        assert_eq!(indexed_count, 2);

        let today = ms_to_ymd_date(system_time_to_ms(SystemTime::now()));
        let notes = query_notes_on_date(&conn, &today, CalendarDateField::Modified).unwrap();
        assert_eq!(notes.len(), 2);

        delete_indexed_path(&conn, "Folder").unwrap();
        let remaining = query_notes_on_date(&conn, &today, CalendarDateField::Modified).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].path, "Welcome.md");
        assert_eq!(remaining[0].preview, "Welcome Note");
    }

    #[test]
    fn rebuild_index_preserves_edit_log_and_uses_latest_edited_ms_over_file_mtime() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();

        fs::write(root.join("SyncedNote.md"), "# Synced Note\nEdited in app").unwrap();
        fs::write(root.join("ExternalNote.md"), "# External Note\nOnly mtime").unwrap();

        let conn = Connection::open_in_memory().unwrap();
        init_index_schema(&conn).unwrap();

        // Record two in-app edits for SyncedNote.md on 2026-05-10 and 2026-05-12
        let may_10_ms = 1_778_400_000_000_i64; // 2026-05-10T08:00:00Z
        let may_12_ms = 1_778_572_800_000_i64; // 2026-05-12T08:00:00Z
        record_edit_log(&conn, "SyncedNote.md", may_10_ms).unwrap();
        record_edit_log(&conn, "SyncedNote.md", may_12_ms).unwrap();

        // Run a full rebuild_index_from_vault
        let count = rebuild_index_from_vault(&conn, root).unwrap();
        assert_eq!(count, 2);

        // Verify edit_log was preserved across the rebuild
        assert_eq!(
            latest_edit_log_ms(&conn, "SyncedNote.md").unwrap(),
            Some(may_12_ms)
        );

        // Verify Calendar Modified query uses the latest edit_log date (2026-05-12) for SyncedNote.md
        let may_days =
            query_calendar_month(&conn, "2026-05", CalendarDateField::Modified).unwrap();
        assert_eq!(
            may_days,
            vec![CalendarDaySummary {
                date: "2026-05-12".to_string(),
                count: 1,
            }]
        );
        let may_12_notes =
            query_notes_on_date(&conn, "2026-05-12", CalendarDateField::Modified).unwrap();
        assert_eq!(may_12_notes.len(), 1);
        assert_eq!(may_12_notes[0].path, "SyncedNote.md");
        assert_eq!(may_12_notes[0].modified_ms, may_12_ms);

        // Verify ExternalNote.md (no edit_log entry) falls back to its filesystem mtime (today)
        let today = ms_to_ymd_date(system_time_to_ms(SystemTime::now()));
        let today_notes =
            query_notes_on_date(&conn, &today, CalendarDateField::Modified).unwrap();
        assert_eq!(today_notes.len(), 1);
        assert_eq!(today_notes[0].path, "ExternalNote.md");
    }

    #[test]
    fn indexes_links_into_sqlite_and_queries_backlinks_incrementally() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();

        fs::create_dir_all(root.join("Specs")).unwrap();
        fs::write(
            root.join("Specs/Clipboard Engine.md"),
            "# Clipboard Engine\nCore spec for #clipboard and #windows11.",
        )
        .unwrap();
        fs::write(
            root.join("Daily.md"),
            "---\ntags: [daily, windows11]\n---\n# Daily Log\nReviewed [[Clipboard Engine#Overview|Clipboard Spec]] today.\nAlso linked [[Specs/Clipboard Engine]] on line 6.",
        )
        .unwrap();

        let conn = Connection::open_in_memory().unwrap();
        rebuild_index_from_vault(&conn, root).unwrap();

        let backlinks = query_backlinks_for_note(&conn, "Specs/Clipboard Engine.md").unwrap();
        assert_eq!(backlinks.len(), 2);
        assert_eq!(backlinks[0].source_path, "Daily.md");
        assert_eq!(backlinks[0].source_title, "Daily");
        assert_eq!(backlinks[0].heading, Some("Overview".to_string()));
        assert_eq!(backlinks[0].alias, Some("Clipboard Spec".to_string()));
        assert_eq!(backlinks[0].line_number, 5);
        assert_eq!(
            backlinks[0].line_context,
            "Reviewed [[Clipboard Engine#Overview|Clipboard Spec]] today."
        );
        assert_eq!(backlinks[1].line_number, 6);

        // Verify tags aggregation across frontmatter and inline #tags
        let tags = query_all_tags(&conn).unwrap();
        assert_eq!(
            tags,
            vec![
                TagSummary {
                    tag: "windows11".to_string(),
                    count: 2
                },
                TagSummary {
                    tag: "clipboard".to_string(),
                    count: 1
                },
                TagSummary {
                    tag: "daily".to_string(),
                    count: 1
                },
            ]
        );

        let win11_notes = query_notes_by_tag(&conn, "windows11").unwrap();
        assert_eq!(win11_notes.len(), 2);

        // Incrementally delete Daily.md and verify backlinks and tags update
        delete_indexed_path(&conn, "Daily.md").unwrap();
        let backlinks_after = query_backlinks_for_note(&conn, "Specs/Clipboard Engine.md").unwrap();
        assert!(backlinks_after.is_empty());
    }

    #[test]
    fn resolves_wiki_links_with_two_plan_md_notes_and_reresolves_on_create_rename_delete() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();

        fs::create_dir_all(root.join("Alpha")).unwrap();
        fs::create_dir_all(root.join("Beta/Deep")).unwrap();
        fs::create_dir_all(root.join("Gamma")).unwrap();

        // Two notes named Plan.md in different folders ("Alpha/Plan.md" is shorter than "Beta/Deep/Plan.md")
        fs::write(root.join("Alpha/Plan.md"), "# Alpha Plan").unwrap();
        fs::write(root.join("Beta/Deep/Plan.md"), "# Beta Deep Plan").unwrap();

        // Sibling in Beta/Deep links to [[Plan]] -> should resolve to same-folder "Beta/Deep/Plan.md"
        fs::write(
            root.join("Beta/Deep/Sibling.md"),
            "Same folder link: [[Plan]]",
        )
        .unwrap();

        // Gamma/Overview.md links to:
        // - [[Plan]] -> should resolve to shortest path "Alpha/Plan.md"
        // - [[Beta/Deep/Plan]] -> should resolve by exact relative path to "Beta/Deep/Plan.md"
        // - [[NonExistent]] -> unresolved (NULL)
        fs::write(
            root.join("Gamma/Overview.md"),
            "Stem link: [[Plan]]\nExact path link: [[Beta/Deep/Plan]]\nMissing: [[NonExistent]]",
        )
        .unwrap();

        let conn = Connection::open_in_memory().unwrap();
        rebuild_index_from_vault(&conn, root).unwrap();

        // Verify unresolved link stores NULL in resolved_path
        let missing_resolved: Option<String> = conn
            .query_row(
                "SELECT resolved_path FROM links WHERE source_path = 'Gamma/Overview.md' AND target = 'NonExistent'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(missing_resolved, None);

        // Alpha/Plan.md gets backlink from Gamma/Overview.md's [[Plan]] (shortest path)
        let alpha_backlinks = query_backlinks_for_note(&conn, "Alpha/Plan.md").unwrap();
        assert_eq!(alpha_backlinks.len(), 1);
        assert_eq!(alpha_backlinks[0].source_path, "Gamma/Overview.md");
        assert_eq!(alpha_backlinks[0].target, "Plan");

        // Beta/Deep/Plan.md gets backlinks from Beta/Deep/Sibling.md (same folder) and Gamma/Overview.md (exact relative path)
        let beta_backlinks = query_backlinks_for_note(&conn, "Beta/Deep/Plan.md").unwrap();
        assert_eq!(beta_backlinks.len(), 2);
        assert_eq!(beta_backlinks[0].source_path, "Beta/Deep/Sibling.md");
        assert_eq!(beta_backlinks[1].source_path, "Gamma/Overview.md");

        // 1. CREATE: Create Gamma/Plan.md in the same folder as Gamma/Overview.md.
        // Gamma/Overview.md's [[Plan]] must re-resolve from Alpha/Plan.md to Gamma/Plan.md!
        let gamma_plan = root.join("Gamma/Plan.md");
        fs::write(&gamma_plan, "# Gamma Plan").unwrap();
        index_single_file(&conn, root, &gamma_plan, None).unwrap();

        let gamma_backlinks = query_backlinks_for_note(&conn, "Gamma/Plan.md").unwrap();
        assert_eq!(gamma_backlinks.len(), 1);
        assert_eq!(gamma_backlinks[0].source_path, "Gamma/Overview.md");
        assert!(query_backlinks_for_note(&conn, "Alpha/Plan.md")
            .unwrap()
            .is_empty());

        // 2. DELETE: Delete Gamma/Plan.md -> Gamma/Overview.md's [[Plan]] re-resolves back to Alpha/Plan.md
        delete_indexed_path(&conn, "Gamma/Plan.md").unwrap();
        let alpha_backlinks_after_del = query_backlinks_for_note(&conn, "Alpha/Plan.md").unwrap();
        assert_eq!(alpha_backlinks_after_del.len(), 1);
        assert_eq!(
            alpha_backlinks_after_del[0].source_path,
            "Gamma/Overview.md"
        );

        // 3. RENAME: Rename Alpha/Plan.md to Archive/LongSubfolder/Deep/Plan.md (longer than Beta/Deep/Plan.md)
        fs::create_dir_all(root.join("Archive/LongSubfolder/Deep")).unwrap();
        let renamed_plan = root.join("Archive/LongSubfolder/Deep/Plan.md");
        fs::write(&renamed_plan, "# Renamed Plan").unwrap();
        delete_indexed_path(&conn, "Alpha/Plan.md").unwrap();
        index_single_file(&conn, root, &renamed_plan, None).unwrap();

        // Now Beta/Deep/Plan.md is the shortest path for [[Plan]], so Gamma/Overview.md's [[Plan]] resolves to Beta/Deep/Plan.md
        let beta_backlinks_after_rename =
            query_backlinks_for_note(&conn, "Beta/Deep/Plan.md").unwrap();
        assert_eq!(beta_backlinks_after_rename.len(), 3);
    }
}
