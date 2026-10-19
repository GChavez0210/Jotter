use rusqlite::{params, Connection};

use crate::types::{CalendarDateField, CalendarDaySummary, CalendarNoteEntry};

/// Initializes the rebuildable SQLite metadata cache schema.
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
        CREATE INDEX IF NOT EXISTS idx_notes_modified_date ON notes(modified_date);
        CREATE INDEX IF NOT EXISTS idx_notes_created_date ON notes(created_date);",
    )
    .map_err(|e| format!("Failed to initialize SQLite index schema: {e}"))
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

/// Queries per-day note counts for a given `year_month` prefix (e.g. `"2026-10"`).
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
}
