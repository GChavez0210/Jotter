//! Task reminders: a system notification when a task with a due date comes due.
//!
//! A background loop reads the open vault's indexed tasks and asks [`due_reminders`] which ones
//! to announce. Which tasks were announced lives in `reminders.json` in the app data folder, so
//! restarting Jotter does not announce them again.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

use chrono::{DateTime, Duration, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};

use crate::fs_atomic::atomic_write_str;
use crate::types::TaskEntry;

/// A poisoned connection may contain an interrupted transaction, and its index stops updating.
/// Rebuild an isolated reminder index from the current vault files instead of trusting stale data.
pub fn query_tasks(connection: &Mutex<rusqlite::Connection>, vault_root: &Path) -> Result<Vec<TaskEntry>, String> {
    match connection.lock() {
        Ok(conn) => crate::index::query_due_tasks(&conn),
        Err(poisoned) => {
            drop(poisoned);
            let files = crate::index::scan_dir_files_from_disk(vault_root, vault_root)?;
            let conn = rusqlite::Connection::open_in_memory().map_err(|e| format!("Cannot rebuild reminder index: {e}"))?;
            crate::index::init_index_schema(&conn)?;
            crate::index::apply_scanned_vault_index(&conn, &files)?;
            crate::index::query_due_tasks(&conn)
        }
    }
}

/// Tasks with a date but no time remind at this hour on the due day.
pub const ALL_DAY_REMINDER_HOUR: u32 = 9;
/// A timed task still reminds this long after its due time, for a computer that was asleep.
const TIMED_GRACE_MINUTES: i64 = 60;
/// Announced entries older than this are dropped from the file.
const KEEP_DAYS: i64 = 3;

/// Whether reminders are on. The frontend sets it from the user's setting.
#[derive(Default)]
pub struct ReminderState {
    pub enabled: AtomicBool,
}

pub struct Reminder {
    /// Identifies the task, so the same one is not announced twice.
    pub key: String,
    pub title: String,
    pub body: String,
    pub due: DateTime<Utc>,
}

pub fn reminders_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("reminders.json")
}

/// The moment a task reminds, or `None` for a malformed date or time.
fn reminder_moment(task: &TaskEntry) -> Option<NaiveDateTime> {
    let date = NaiveDate::parse_from_str(&task.due_date, "%Y-%m-%d").ok()?;
    let time = match task.due_time.as_deref() {
        Some(raw) => NaiveTime::parse_from_str(raw, "%H:%M").ok()?,
        None => NaiveTime::from_hms_opt(ALL_DAY_REMINDER_HOUR, 0, 0)?,
    };
    Some(date.and_time(time))
}

/// Repeated wall times use their first occurrence. Skipped wall times move to the first
/// valid minute after the clock gap. Grace periods always measure elapsed UTC time.
fn resolve_moment<T: TimeZone>(zone: &T, mut wall: NaiveDateTime) -> Option<DateTime<Utc>> {
    for _ in 0..=180 {
        match zone.from_local_datetime(&wall) {
            LocalResult::Single(time) => return Some(time.with_timezone(&Utc)),
            LocalResult::Ambiguous(first, second) => return Some(first.min(second).with_timezone(&Utc)),
            LocalResult::None => wall = wall.checked_add_signed(Duration::minutes(1))?,
        }
    }
    None
}

fn task_key(vault_key: &str, task: &TaskEntry) -> String {
    format!("{vault_key}\n{}\n{}\n{}\n{}", task.path, task.text, task.due_date,
        task.due_time.as_deref().unwrap_or(""))
}

/// Selects open tasks from one vault. A closed vault has no reminder scope.
pub fn due_reminders<T: TimeZone>(
    vault_key: Option<&str>,
    tasks: &[TaskEntry],
    now: DateTime<T>,
    announced: &HashMap<String, DateTime<Utc>>,
) -> Vec<Reminder> {
    let Some(vault_key) = vault_key else { return Vec::new() };
    let mut seen = std::collections::HashSet::new();
    let utc_now = now.with_timezone(&Utc);
    tasks.iter().filter(|task| !task.done).filter_map(|task| {
        let wall = reminder_moment(task)?;
        let moment = resolve_moment(&now.timezone(), wall)?;
        if utc_now < moment { return None; }
        if task.due_time.is_some() {
            if utc_now - moment > Duration::minutes(TIMED_GRACE_MINUTES) { return None; }
        } else if now.date_naive() != wall.date() { return None; }
        let key = task_key(vault_key, task);
        if announced.contains_key(&key) || !seen.insert(key.clone()) { return None; }
        let body = if task.text.trim().is_empty() { "Task".to_string() } else { task.text.clone() };
        Some(Reminder { key, title: task.title.clone(), body, due: moment })
    }).collect()
}

/// Pruning also runs when reminders are off or no new tasks are due.
pub fn prune_announced(announced: &mut HashMap<String, DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    let before = announced.len();
    announced.retain(|_, due| now - *due <= Duration::days(KEEP_DAYS));
    announced.len() != before
}

/// Reads the announced entries. A missing or damaged file means nothing was announced.
pub fn load_announced(file: &Path) -> HashMap<String, DateTime<Utc>> {
    let Ok(raw) = std::fs::read_to_string(file) else {
        return HashMap::new();
    };
    let Ok(stored) = serde_json::from_str::<HashMap<String, String>>(&raw) else {
        return HashMap::new();
    };
    stored
        .into_iter()
        .filter_map(|(key, due)| {
            DateTime::parse_from_rfc3339(&due).ok().map(|due| (key, due.with_timezone(&Utc)))
        })
        .collect()
}

/// Drops entries whose due time is past the keep window, then writes the rest.
pub fn save_announced(
    file: &Path,
    announced: &mut HashMap<String, DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Result<(), String> {
    prune_announced(announced, now);
    let stored: HashMap<&String, String> = announced
        .iter()
        .map(|(key, due)| (key, due.to_rfc3339()))
        .collect();
    let json = serde_json::to_string_pretty(&stored)
        .map_err(|e| format!("Cannot serialize reminders: {e}"))?;
    atomic_write_str(file, &json)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> DateTime<Utc> {
        NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M").unwrap().and_utc()
    }

    fn due_reminders(tasks: &[TaskEntry], now: DateTime<Utc>, announced: &HashMap<String, DateTime<Utc>>) -> Vec<Reminder> {
        super::due_reminders(Some("vault-a"), tasks, now, announced)
    }

    fn task(text: &str, date: &str, time: Option<&str>, done: bool) -> TaskEntry {
        TaskEntry {
            path: "Plan.md".into(),
            title: "Plan".into(),
            line_number: 1,
            text: text.into(),
            done,
            due_date: date.into(),
            due_time: time.map(String::from),
        }
    }

    #[test]
    fn a_timed_task_reminds_from_its_time_until_the_grace_period_ends() {
        let tasks = [task("Call Robert", "2026-10-30", Some("14:30"), false)];
        let none = HashMap::new();
        assert!(due_reminders(&tasks, at("2026-10-30 14:29"), &none).is_empty());
        assert_eq!(due_reminders(&tasks, at("2026-10-30 14:30"), &none).len(), 1);
        assert_eq!(due_reminders(&tasks, at("2026-10-30 15:30"), &none).len(), 1);
        assert!(due_reminders(&tasks, at("2026-10-30 15:31"), &none).is_empty());
    }

    #[test]
    fn an_all_day_task_reminds_at_nine_on_its_own_day_only() {
        let tasks = [task("Pay rent", "2026-10-30", None, false)];
        let none = HashMap::new();
        assert!(due_reminders(&tasks, at("2026-10-30 08:59"), &none).is_empty());
        assert_eq!(due_reminders(&tasks, at("2026-10-30 09:00"), &none).len(), 1);
        assert_eq!(due_reminders(&tasks, at("2026-10-30 23:59"), &none).len(), 1);
        assert!(due_reminders(&tasks, at("2026-10-31 09:00"), &none).is_empty());
    }

    #[test]
    fn done_announced_and_malformed_tasks_stay_quiet() {
        let now = at("2026-10-30 10:00");
        let open = task("Pay rent", "2026-10-30", None, false);
        let mut announced = HashMap::new();
        announced.insert(task_key("vault-a", &open), at("2026-10-30 09:00"));
        let tasks = [
            task("Done thing", "2026-10-30", None, true),
            open,
            task("Bad date", "2026-13-40", None, false),
            task("Bad time", "2026-10-30", Some("25:00"), false),
        ];
        assert!(due_reminders(&tasks, now, &announced).is_empty());
    }

    #[test]
    fn duplicate_tasks_announce_once() {
        let tasks = [
            task("Pay rent", "2026-10-30", None, false),
            task("Pay rent", "2026-10-30", None, false),
        ];
        assert_eq!(due_reminders(&tasks, at("2026-10-30 10:00"), &HashMap::new()).len(), 1);
    }

    #[test]
    fn closed_vaults_stay_quiet_and_vaults_do_not_share_announcements() {
        let tasks = [task("Call", "2026-10-30", Some("10:00"), false)];
        let now = at("2026-10-30 10:00");
        assert!(super::due_reminders(None, &tasks, now, &HashMap::new()).is_empty());
        let first = super::due_reminders(Some("vault-a"), &tasks, now, &HashMap::new());
        let announced = HashMap::from([(first[0].key.clone(), first[0].due)]);
        assert!(super::due_reminders(Some("vault-a"), &tasks, now, &announced).is_empty());
        assert_eq!(super::due_reminders(Some("vault-b"), &tasks, now, &announced).len(), 1);
    }

    #[test]
    fn a_quiet_cycle_prunes_old_history_without_new_due_tasks() {
        let mut history = HashMap::from([("old".into(), at("2026-10-20 09:00"))]);
        let now = at("2026-10-30 10:00");
        assert!(due_reminders(&[], now, &history).is_empty());
        assert!(prune_announced(&mut history, now));
        assert!(history.is_empty());
        assert!(!prune_announced(&mut history, now));
    }

    #[test]
    fn a_poisoned_connection_reads_current_tasks_without_reusing_a_stale_index() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Plan.md");
        std::fs::write(&file, "- [ ] Call 📅 2026-10-30 10:00").unwrap();
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::index::init_index_schema(&conn).unwrap();
        crate::index::upsert_scanned_note(&conn, &crate::index::ScannedNoteFile {
            path: "Plan.md".into(), title: "Plan".into(), fs_modified_ms: 0, created_ms: 0,
            preview: String::new(), content: "- [ ] Call 📅 2026-10-30 10:00".into(), links: vec![], tags: vec![],
        }, None).unwrap();
        let shared = Mutex::new(conn);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = shared.lock().unwrap();
            panic!("interrupt index owner");
        }));
        assert!(shared.is_poisoned());
        let tasks = query_tasks(&shared, dir.path()).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].text, "Call");
        std::fs::write(&file, "- [x] Call 📅 2026-10-30 10:00").unwrap();
        assert!(query_tasks(&shared, dir.path()).unwrap()[0].done);
        assert!(shared.is_poisoned());
    }

    // Denver's 2026 transitions, expressed as a deterministic zone so tests do not depend
    // on the host timezone or change the system clock.
    #[derive(Clone)]
    struct TestZone;
    impl TimeZone for TestZone {
        type Offset = chrono::FixedOffset;
        fn from_offset(_: &Self::Offset) -> Self { Self }
        fn offset_from_local_date(&self, date: &NaiveDate) -> LocalResult<Self::Offset> {
            self.offset_from_local_datetime(&date.and_hms_opt(12, 0, 0).unwrap())
        }
        fn offset_from_local_datetime(&self, wall: &NaiveDateTime) -> LocalResult<Self::Offset> {
            let standard = chrono::FixedOffset::west_opt(7 * 3600).unwrap();
            let daylight = chrono::FixedOffset::west_opt(6 * 3600).unwrap();
            let spring = at("2026-03-08 02:00").naive_utc();
            let fall = at("2026-11-01 01:00").naive_utc();
            if *wall >= spring && *wall < spring + Duration::hours(1) { LocalResult::None }
            else if *wall >= fall && *wall < fall + Duration::hours(1) { LocalResult::Ambiguous(daylight, standard) }
            else { LocalResult::Single(if *wall >= spring && *wall < fall { daylight } else { standard }) }
        }
        fn offset_from_utc_date(&self, date: &NaiveDate) -> Self::Offset {
            self.offset_from_utc_datetime(&date.and_hms_opt(12, 0, 0).unwrap())
        }
        fn offset_from_utc_datetime(&self, utc: &NaiveDateTime) -> Self::Offset {
            chrono::FixedOffset::west_opt(if *utc >= at("2026-03-08 09:00").naive_utc()
                && *utc < at("2026-11-01 08:00").naive_utc() { 6 * 3600 } else { 7 * 3600 }).unwrap()
        }
    }

    #[test]
    fn dst_grace_uses_elapsed_time_and_gap_tasks_use_the_first_valid_minute() {
        let spring = [task("Before gap", "2026-03-08", Some("01:50"), false),
            task("In gap", "2026-03-08", Some("02:15"), false)];
        let now = TestZone.from_utc_datetime(&at("2026-03-08 09:00").naive_utc());
        let due = super::due_reminders(Some("vault-a"), &spring, now, &HashMap::new());
        assert_eq!(due.len(), 2);
        assert_eq!(due[1].due, at("2026-03-08 09:00"));
        let fall = [task("Before rollback", "2026-11-01", Some("01:50"), false)];
        let now = TestZone.from_utc_datetime(&at("2026-11-01 08:30").naive_utc());
        assert_eq!(super::due_reminders(Some("vault-a"), &fall, now, &HashMap::new()).len(), 1);
        let late = TestZone.from_utc_datetime(&at("2026-11-01 09:00").naive_utc());
        assert!(super::due_reminders(Some("vault-a"), &fall, late, &HashMap::new()).is_empty());
    }

    #[test]
    fn announced_entries_survive_a_restart_and_old_ones_are_pruned() {
        let dir = std::env::temp_dir().join(format!("jotter-reminders-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = reminders_path(&dir);
        let mut announced = HashMap::new();
        announced.insert("fresh".to_string(), at("2026-10-30 09:00"));
        announced.insert("stale".to_string(), at("2026-10-20 09:00"));
        save_announced(&file, &mut announced, at("2026-10-30 10:00")).unwrap();
        let loaded = load_announced(&file);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.get("fresh"), Some(&at("2026-10-30 09:00")));
    }
}
