//! Checklist items with due dates, and the `.ics` calendar file built from them.
//!
//! A task is a list item with a checkbox (`- [ ]`, `* [x]`, `1. [ ]`). Its due date uses one of
//! three common syntaxes, each with an optional `HH:MM` time:
//! - `📅 2026-10-30` (Obsidian Tasks)
//! - `due:2026-10-30` (todo.txt)
//! - `[due:: 2026-10-30]` (Dataview inline field)

use crate::index::ms_to_ymd_date;
use crate::links::{replace_wiki_links_with_display, split_yaml_frontmatter};
use crate::types::TaskEntry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueTask {
    pub line_number: usize,
    pub text: String,
    pub done: bool,
    pub due_date: String,
    pub due_time: Option<String>,
}

/// A due date found in a line: the byte range it occupies and its parsed parts.
struct DueMatch {
    start: usize,
    end: usize,
    date: String,
    time: Option<String>,
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        _ => 0,
    }
}

/// Parses `YYYY-MM-DD` at the start of `s`, then an optional `HH:MM` after a space or `T`.
/// Returns the date, the time, and the number of bytes consumed.
fn parse_date_at(s: &str) -> Option<(String, Option<String>, usize)> {
    let b = s.as_bytes();
    if b.len() < 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let digits = |r: std::ops::Range<usize>| -> Option<u32> {
        let part = s.get(r)?;
        part.bytes().all(|c| c.is_ascii_digit()).then(|| part.parse().ok())?
    };
    let (year, month, day) = (digits(0..4)?, digits(5..7)?, digits(8..10)?);
    if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
        return None;
    }
    // A date glued to more digits, such as `2026-10-301`, is not a date.
    if b.get(10).is_some_and(|c| c.is_ascii_digit()) {
        return None;
    }
    let date = s[..10].to_string();
    if b.len() >= 16 && (b[10] == b' ' || b[10] == b'T') && b[13] == b':' {
        if let (Some(h), Some(m)) = (digits(11..13), digits(14..16)) {
            let followed_by_digit = b.get(16).is_some_and(|c| c.is_ascii_digit());
            if h < 24 && m < 60 && !followed_by_digit {
                return Some((date, Some(format!("{h:02}:{m:02}")), 16));
            }
        }
    }
    Some((date, None, 10))
}

fn find_due(line: &str) -> Option<DueMatch> {
    // 📅 2026-10-30
    if let Some(pos) = line.find('📅') {
        let after = pos + '📅'.len_utf8();
        let rest = &line[after..];
        let skipped = rest.len() - rest.trim_start_matches([' ', '\u{fe0f}']).len();
        if let Some((date, time, used)) = parse_date_at(&rest[skipped..]) {
            return Some(DueMatch { start: pos, end: after + skipped + used, date, time });
        }
    }
    // [due:: 2026-10-30]
    let lower = line.to_ascii_lowercase();
    if let Some(pos) = lower.find("[due::") {
        let rest = &line[pos + 6..];
        let skipped = rest.len() - rest.trim_start().len();
        if let Some((date, time, used)) = parse_date_at(&rest[skipped..]) {
            let after = pos + 6 + skipped + used;
            let close = line[after..].find(']').map(|i| after + i + 1);
            if let Some(end) = close.filter(|&e| line[after..e - 1].trim().is_empty()) {
                return Some(DueMatch { start: pos, end, date, time });
            }
        }
    }
    // due:2026-10-30, not inside a longer word such as `overdue:`
    let mut from = 0;
    while let Some(rel) = lower[from..].find("due:") {
        let pos = from + rel;
        let boundary = pos == 0 || !line.as_bytes()[pos - 1].is_ascii_alphanumeric();
        if boundary {
            if let Some((date, time, used)) = parse_date_at(&line[pos + 4..]) {
                return Some(DueMatch { start: pos, end: pos + 4 + used, date, time });
            }
        }
        from = pos + 4;
    }
    None
}

/// Returns the checkbox state and the text after the checkbox, or `None` for other lines.
fn task_checkbox(line: &str) -> Option<(bool, &str)> {
    let trimmed = line.trim_start();
    let after_marker = if let Some(rest) = trimmed.strip_prefix(['-', '*', '+']) {
        rest
    } else {
        let digits = trimmed.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 || digits > 9 {
            return None;
        }
        trimmed[digits..].strip_prefix(['.', ')'])?
    };
    let rest = after_marker.strip_prefix([' ', '\t'])?.trim_start();
    let b = rest.as_bytes();
    if b.len() < 3 || b[0] != b'[' || b[2] != b']' {
        return None;
    }
    let done = match b[1] {
        b' ' => false,
        b'x' | b'X' => true,
        // `[-]` (cancelled) and other custom states are not open or done tasks.
        _ => return None,
    };
    Some((done, &rest[3..]))
}

/// Cleans task text for a calendar title: removes the due date, shows wiki links by their
/// display text, and drops emphasis markers.
fn summary_text(text: &str) -> String {
    let display = replace_wiki_links_with_display(text);
    let cleaned: String = display.replace("**", "").replace("__", "").replace('`', "");
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Finds open and completed tasks that have a due date, skipping frontmatter and code blocks.
/// Line numbers are 1-based and count frontmatter lines, matching the links table.
pub fn parse_due_tasks(content: &str) -> Vec<DueTask> {
    let (body, frontmatter) = split_yaml_frontmatter(content);
    let offset = frontmatter.map_or(0, |fm| if fm.is_empty() { 2 } else { fm.lines().count() + 2 });
    let mut tasks = Vec::new();
    let mut in_fence = false;
    for (idx, line) in body.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let Some((done, rest)) = task_checkbox(line) else { continue };
        let Some(due) = find_due(rest) else { continue };
        let text = format!("{} {}", &rest[..due.start], &rest[due.end..]);
        tasks.push(DueTask {
            line_number: offset + idx + 1,
            text: summary_text(&text),
            done,
            due_date: due.date,
            due_time: due.time,
        });
    }
    tasks
}

/// Escapes text for an iCalendar TEXT value (RFC 5545, section 3.3.11).
fn ics_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            _ => out.push(ch),
        }
    }
    out
}

/// Folds a content line at 75 octets without splitting a UTF-8 character (RFC 5545, 3.1).
fn fold_line(line: &str, out: &mut String) {
    let mut width = 0;
    for ch in line.chars() {
        let len = ch.len_utf8();
        if width + len > 75 {
            out.push_str("\r\n ");
            width = 1;
        }
        out.push(ch);
        width += len;
    }
    out.push_str("\r\n");
}

fn fnv1a_64(input: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn compact_date(date: &str) -> String {
    date.replace('-', "")
}

/// The day after `YYYY-MM-DD`, for an all-day event's exclusive end date.
fn next_day(date: &str) -> String {
    let year: u32 = date[0..4].parse().unwrap_or(1970);
    let month: u32 = date[5..7].parse().unwrap_or(1);
    let day: u32 = date[8..10].parse().unwrap_or(1);
    let (y, m, d) = if day < days_in_month(year, month) {
        (year, month, day + 1)
    } else if month < 12 {
        (year, month + 1, 1)
    } else {
        (year + 1, 1, 1)
    };
    format!("{y:04}{m:02}{d:02}")
}

fn utc_stamp(now_ms: i64) -> String {
    let secs = now_ms.div_euclid(1000);
    let time = secs.rem_euclid(86_400);
    format!(
        "{}T{:02}{:02}{:02}Z",
        compact_date(&ms_to_ymd_date(now_ms)),
        time / 3600,
        (time % 3600) / 60,
        time % 60
    )
}

/// Builds an iCalendar file with one event per open task. Dates without a time become all-day
/// events; times are floating local times, so they show at that hour in any time zone.
/// `now_ms` only sets `DTSTAMP`, which calendar apps use to spot changed events.
pub fn build_ics(calendar_name: &str, tasks: &[TaskEntry], now_ms: i64) -> String {
    let mut out = String::new();
    let stamp = utc_stamp(now_ms);
    for line in [
        "BEGIN:VCALENDAR",
        "VERSION:2.0",
        "PRODID:-//Jotter//Task due dates//EN",
        "CALSCALE:GREGORIAN",
        "METHOD:PUBLISH",
    ] {
        fold_line(line, &mut out);
    }
    fold_line(&format!("X-WR-CALNAME:{}", ics_escape(calendar_name)), &mut out);

    for task in tasks.iter().filter(|t| !t.done) {
        let uid = fnv1a_64(&format!("{}\n{}\n{}", task.path, task.text, task.due_date));
        fold_line("BEGIN:VEVENT", &mut out);
        fold_line(&format!("UID:{uid:016x}@jotter"), &mut out);
        fold_line(&format!("DTSTAMP:{stamp}"), &mut out);
        let date = compact_date(&task.due_date);
        match &task.due_time {
            Some(time) => {
                fold_line(&format!("DTSTART:{date}T{}00", time.replace(':', "")), &mut out);
                fold_line("DURATION:PT30M", &mut out);
            }
            None => {
                fold_line(&format!("DTSTART;VALUE=DATE:{date}"), &mut out);
                fold_line(&format!("DTEND;VALUE=DATE:{}", next_day(&task.due_date)), &mut out);
            }
        }
        let summary = if task.text.is_empty() { "Task" } else { task.text.as_str() };
        fold_line(&format!("SUMMARY:{}", ics_escape(summary)), &mut out);
        fold_line(
            &format!("DESCRIPTION:{}", ics_escape(&format!("{} (line {})", task.path, task.line_number))),
            &mut out,
        );
        fold_line("TRANSP:TRANSPARENT", &mut out);
        fold_line("END:VEVENT", &mut out);
    }
    fold_line("END:VCALENDAR", &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(path: &str, text: &str, date: &str, time: Option<&str>, done: bool) -> TaskEntry {
        TaskEntry {
            path: path.into(),
            title: path.trim_end_matches(".md").into(),
            line_number: 3,
            text: text.into(),
            done,
            due_date: date.into(),
            due_time: time.map(Into::into),
        }
    }

    #[test]
    fn parses_three_due_syntaxes_and_skips_code_and_non_tasks() {
        let doc = "---\ntags: [a]\n---\n- [ ] Pay rent 📅 2026-11-01\n* [x] Call [[Bob|Robert]] due:2026-10-20 09:30\n1. [ ] Review [due:: 2026-10-31] notes\n- [-] Cancelled 📅 2026-10-01\n- Plain item due:2026-10-02\n```\n- [ ] Code 📅 2026-10-03\n```\n- [ ] overdue:2026-10-04 is not a due date\n- [ ] Bad date 📅 2026-02-30\n";
        let tasks = parse_due_tasks(doc);
        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[0].line_number, 4);
        assert_eq!(tasks[0].text, "Pay rent");
        assert_eq!(tasks[0].due_date, "2026-11-01");
        assert!(!tasks[0].done);
        assert_eq!(tasks[1].text, "Call Robert");
        assert_eq!(tasks[1].due_time.as_deref(), Some("09:30"));
        assert!(tasks[1].done);
        assert_eq!(tasks[2].text, "Review notes");
        assert_eq!(tasks[2].due_date, "2026-10-31");
    }

    #[test]
    fn ics_has_all_day_and_timed_events_and_skips_done_tasks() {
        let tasks = vec![
            task("Home.md", "Pay rent, now; really", "2026-12-31", None, false),
            task("Work.md", "Standup", "2026-10-20", Some("09:30"), false),
            task("Work.md", "Old", "2026-10-01", None, true),
        ];
        let ics = build_ics("Notes", &tasks, 1_792_411_000_000);
        assert!(ics.starts_with("BEGIN:VCALENDAR\r\n"));
        assert!(ics.ends_with("END:VCALENDAR\r\n"));
        assert_eq!(ics.matches("BEGIN:VEVENT").count(), 2);
        assert!(ics.contains("DTSTART;VALUE=DATE:20261231\r\nDTEND;VALUE=DATE:20270101"));
        assert!(ics.contains("DTSTART:20261020T093000\r\nDURATION:PT30M"));
        assert!(ics.contains("SUMMARY:Pay rent\\, now\\; really"));
        assert!(!ics.contains("SUMMARY:Old"));
        assert!(ics.lines().all(|l| l.len() <= 75));
    }

    #[test]
    fn long_lines_fold_on_character_boundaries() {
        let mut out = String::new();
        fold_line(&format!("SUMMARY:{}", "é".repeat(60)), &mut out);
        for line in out.split("\r\n").filter(|l| !l.is_empty()) {
            assert!(line.len() <= 75);
        }
        assert_eq!(out.replace("\r\n ", "").trim_end(), format!("SUMMARY:{}", "é".repeat(60)));
    }
}
