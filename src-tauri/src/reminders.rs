//! Task reminders: a system notification when a task with a due date comes due.
//!
//! A background loop reads the open vault's indexed tasks and asks [`due_reminders`] which ones
//! to announce. Which tasks were announced lives in `reminders.json` in the app data folder, so
//! restarting Jotter does not announce them again.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime};

use crate::fs_atomic::atomic_write_str;
use crate::types::TaskEntry;

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
    pub due: NaiveDateTime,
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

/// Whether `moment` has arrived and is still worth announcing at `now`. A timed task has a grace
/// period. An all-day task reminds only on its own day.
fn in_window(task: &TaskEntry, moment: NaiveDateTime, now: NaiveDateTime) -> bool {
    if now < moment {
        return false;
    }
    if task.due_time.is_some() {
        now - moment <= Duration::minutes(TIMED_GRACE_MINUTES)
    } else {
        now.date() == moment.date()
    }
}

fn task_key(task: &TaskEntry) -> String {
    format!(
        "{}\n{}\n{}\n{}",
        task.path,
        task.text,
        task.due_date,
        task.due_time.as_deref().unwrap_or("")
    )
}

/// The open tasks to announce at `now`, skipping done tasks and the keys in `announced`.
pub fn due_reminders(
    tasks: &[TaskEntry],
    now: NaiveDateTime,
    announced: &HashMap<String, NaiveDateTime>,
) -> Vec<Reminder> {
    let mut seen = std::collections::HashSet::new();
    tasks
        .iter()
        .filter(|task| !task.done)
        .filter_map(|task| {
            let moment = reminder_moment(task)?;
            if !in_window(task, moment, now) {
                return None;
            }
            let key = task_key(task);
            if announced.contains_key(&key) || !seen.insert(key.clone()) {
                return None;
            }
            let body = if task.text.trim().is_empty() { "Task".to_string() } else { task.text.clone() };
            Some(Reminder { key, title: task.title.clone(), body, due: moment })
        })
        .collect()
}

/// Reads the announced entries. A missing or damaged file means nothing was announced.
pub fn load_announced(file: &Path) -> HashMap<String, NaiveDateTime> {
    let Ok(raw) = std::fs::read_to_string(file) else {
        return HashMap::new();
    };
    let Ok(stored) = serde_json::from_str::<HashMap<String, String>>(&raw) else {
        return HashMap::new();
    };
    stored
        .into_iter()
        .filter_map(|(key, due)| {
            NaiveDateTime::parse_from_str(&due, "%Y-%m-%dT%H:%M:%S").ok().map(|due| (key, due))
        })
        .collect()
}

/// Drops entries whose due time is past the keep window, then writes the rest.
pub fn save_announced(
    file: &Path,
    announced: &mut HashMap<String, NaiveDateTime>,
    now: NaiveDateTime,
) -> Result<(), String> {
    announced.retain(|_, due| now - *due <= Duration::days(KEEP_DAYS));
    let stored: HashMap<&String, String> = announced
        .iter()
        .map(|(key, due)| (key, due.format("%Y-%m-%dT%H:%M:%S").to_string()))
        .collect();
    let json = serde_json::to_string_pretty(&stored)
        .map_err(|e| format!("Cannot serialize reminders: {e}"))?;
    atomic_write_str(file, &json)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M").unwrap()
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
        announced.insert(task_key(&open), at("2026-10-30 09:00"));
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
