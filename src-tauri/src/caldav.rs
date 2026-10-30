//! One-way push of task due dates to a CalDAV calendar.
//!
//! Each open task becomes one event resource named after its UID. Jotter remembers a hash of
//! every body it pushed, per vault in the app data folder, so a sync sends only new and changed
//! events and deletes events whose task was completed or removed. Jotter never reads the
//! calendar back, and it deletes only resources it created itself.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use url::Url;

use crate::fs_atomic::atomic_write_str;

/// What the last successful sync left on the server: event UID to a hash of its body.
#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Debug)]
pub struct PushedState {
    /// The collection the hashes belong to. Pointing Jotter at another calendar starts fresh.
    pub collection: String,
    pub events: BTreeMap<String, String>,
}

#[derive(Serialize, Default, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub created_or_updated: usize,
    pub deleted: usize,
    pub failed: usize,
    pub first_error: Option<String>,
}

#[derive(Debug, PartialEq)]
pub struct SyncPlan {
    /// UIDs to `PUT`: new events and events whose body changed.
    pub put: Vec<String>,
    /// UIDs to `DELETE`: pushed earlier, no longer wanted.
    pub delete: Vec<String>,
}

pub fn state_path(app_data_dir: &Path, vault_root: &Path) -> PathBuf {
    app_data_dir
        .join("vaults")
        .join(format!("{}.caldav.json", crate::platform::vault_key(vault_root)))
}

pub fn load_state(file: &Path) -> PushedState {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save_state(file: &Path, state: &PushedState) -> Result<(), String> {
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Cannot create the Jotter vault settings folder: {e}"))?;
    }
    let json = serde_json::to_string_pretty(state).map_err(|e| format!("Cannot serialize CalDAV state: {e}"))?;
    atomic_write_str(file, &json)
}

/// Checks the calendar address and returns it with a trailing slash, so a resource name joins
/// onto it. Plain `http` is allowed only for a calendar on this computer.
pub fn normalize_collection(raw: &str) -> Result<Url, String> {
    let mut url = Url::parse(raw.trim()).map_err(|_| "Enter the full calendar address, starting with https://".to_string())?;
    let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    match url.scheme() {
        "https" => {}
        "http" if local => {}
        _ => return Err("The calendar address must start with https://".to_string()),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Remove the user name and password from the address and enter them in their own fields".to_string());
    }
    url.set_query(None);
    url.set_fragment(None);
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    Ok(url)
}

/// Hashes an event body without its `DTSTAMP`, which changes on every build.
pub fn body_hash(body: &str) -> String {
    let stable: String = body.lines().filter(|l| !l.starts_with("DTSTAMP:")).collect::<Vec<_>>().join("\n");
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in stable.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Compares the events wanted now with what the last sync pushed.
pub fn plan(desired: &BTreeMap<String, String>, pushed: &BTreeMap<String, String>) -> SyncPlan {
    let put = desired.iter().filter(|(uid, hash)| pushed.get(*uid) != Some(*hash)).map(|(uid, _)| uid.clone()).collect();
    let delete = pushed.keys().filter(|uid| !desired.contains_key(*uid)).cloned().collect();
    SyncPlan { put, delete }
}

fn resource_url(collection: &Url, uid: &str) -> Result<Url, String> {
    collection.join(&format!("{uid}.ics")).map_err(|e| format!("Bad event address: {e}"))
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Cannot start the network client: {e}"))
}

fn describe(status: reqwest::StatusCode) -> String {
    match status.as_u16() {
        401 | 403 => "The server refused the user name or password".to_string(),
        404 => "The calendar address was not found".to_string(),
        code => format!("The server answered with status {code}"),
    }
}

/// Pushes the plan. `events` maps each UID to its body. Returns the state to remember: it
/// holds only events the server confirmed, so a failed event is retried on the next sync.
/// A refused login or a missing calendar stops the run, since every later request would fail
/// the same way.
pub async fn sync(
    collection: &Url,
    username: &str,
    password: &str,
    events: &BTreeMap<String, String>,
    previous: &PushedState,
) -> Result<(PushedState, SyncReport), String> {
    let client = http_client()?;
    let desired: BTreeMap<String, String> = events.iter().map(|(uid, body)| (uid.clone(), body_hash(body))).collect();
    // A different calendar has none of the old events, so treat everything as new.
    let pushed = if previous.collection == collection.as_str() { previous.events.clone() } else { BTreeMap::new() };
    let work = plan(&desired, &pushed);

    let mut next = pushed;
    let mut report = SyncReport::default();
    let fail = |report: &mut SyncReport, message: String| {
        report.failed += 1;
        report.first_error.get_or_insert(message);
    };

    for uid in &work.put {
        let url = resource_url(collection, uid)?;
        let result = client
            .put(url)
            .basic_auth(username, Some(password))
            .header("Content-Type", "text/calendar; charset=utf-8")
            .body(events[uid].clone())
            .send()
            .await;
        match result {
            Ok(resp) if resp.status().is_success() => {
                next.insert(uid.clone(), desired[uid].clone());
                report.created_or_updated += 1;
            }
            Ok(resp) if matches!(resp.status().as_u16(), 401 | 403 | 404) => {
                fail(&mut report, describe(resp.status()));
                break;
            }
            Ok(resp) => fail(&mut report, describe(resp.status())),
            Err(e) => fail(&mut report, format!("Cannot reach the calendar server: {e}")),
        }
    }

    if report.failed == 0 {
        for uid in &work.delete {
            let url = resource_url(collection, uid)?;
            match client.delete(url).basic_auth(username, Some(password)).send().await {
                // Already gone counts as deleted.
                Ok(resp) if resp.status().is_success() || resp.status().as_u16() == 404 => {
                    next.remove(uid);
                    report.deleted += 1;
                }
                Ok(resp) if matches!(resp.status().as_u16(), 401 | 403) => {
                    fail(&mut report, describe(resp.status()));
                    break;
                }
                Ok(resp) => fail(&mut report, describe(resp.status())),
                Err(e) => fail(&mut report, format!("Cannot reach the calendar server: {e}")),
            }
        }
    }

    Ok((PushedState { collection: collection.as_str().to_string(), events: next }, report))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn plan_sends_new_and_changed_events_and_deletes_dropped_ones() {
        let desired = map(&[("a", "1"), ("b", "2"), ("c", "9")]);
        let pushed = map(&[("b", "2"), ("c", "3"), ("d", "4")]);
        let work = plan(&desired, &pushed);
        assert_eq!(work.put, ["a", "c"]);
        assert_eq!(work.delete, ["d"]);
    }

    #[test]
    fn plan_is_empty_when_nothing_changed() {
        let same = map(&[("a", "1")]);
        assert_eq!(plan(&same, &same), SyncPlan { put: vec![], delete: vec![] });
    }

    #[test]
    fn body_hash_ignores_the_timestamp_only() {
        let a = "BEGIN:VEVENT\r\nDTSTAMP:20261001T000000Z\r\nSUMMARY:Pay\r\nEND:VEVENT\r\n";
        let b = "BEGIN:VEVENT\r\nDTSTAMP:20261002T120000Z\r\nSUMMARY:Pay\r\nEND:VEVENT\r\n";
        let c = "BEGIN:VEVENT\r\nDTSTAMP:20261001T000000Z\r\nSUMMARY:Rent\r\nEND:VEVENT\r\n";
        assert_eq!(body_hash(a), body_hash(b));
        assert_ne!(body_hash(a), body_hash(c));
    }

    #[test]
    fn collection_address_gets_a_trailing_slash_and_loses_query_and_fragment() {
        let url = normalize_collection(" https://cal.example.com/dav/me/tasks?x=1#f ").unwrap();
        assert_eq!(url.as_str(), "https://cal.example.com/dav/me/tasks/");
        assert_eq!(resource_url(&url, "ab@jotter").unwrap().as_str(), "https://cal.example.com/dav/me/tasks/ab@jotter.ics");
    }

    #[test]
    fn collection_address_rejects_plain_http_and_embedded_passwords() {
        assert!(normalize_collection("http://cal.example.com/dav/").is_err());
        assert!(normalize_collection("http://localhost:5232/me/").is_ok());
        assert!(normalize_collection("https://me:secret@cal.example.com/dav/").is_err());
        assert!(normalize_collection("not a url").is_err());
        assert!(normalize_collection("ftp://cal.example.com/").is_err());
    }

    #[test]
    fn state_round_trips_and_a_missing_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let file = state_path(dir.path(), &dir.path().join("vault"));
        assert_eq!(load_state(&file), PushedState::default());
        let state = PushedState { collection: "https://x/".into(), events: map(&[("a", "1")]) };
        save_state(&file, &state).unwrap();
        assert_eq!(load_state(&file), state);
    }
    #[tokio::test]
    async fn delete_stops_on_auth_failure_without_forgetting_unsent_events() {
        use std::io::{BufRead, Write};
        use std::net::TcpListener;
        for status in [401, 403] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let collection = normalize_collection(&format!("http://{}/calendar/", listener.local_addr().unwrap())).unwrap();
            let server = std::thread::spawn(move || {
                let until = std::time::Instant::now() + Duration::from_secs(1);
                let mut count = 0;
                while std::time::Instant::now() < until {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                            {
                                let mut reader = std::io::BufReader::new(&mut stream);
                                loop {
                                    let mut line = String::new();
                                    let count = reader.read_line(&mut line).unwrap();
                                    if count == 0 || line == "\r\n" { break; }
                                }
                            }
                            let code = if count == 0 { status } else { 204 };
                            write!(stream, "HTTP/1.1 {code} Test\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                            count += 1;
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(5)),
                        Err(e) => panic!("{e}"),
                    }
                }
                count
            });
            let previous = PushedState { collection: collection.to_string(), events: map(&[("a", "1"), ("b", "2")]) };
            let (next, report) = sync(&collection, "test", "test", &BTreeMap::new(), &previous).await.unwrap();
            assert_eq!(server.join().unwrap(), 1);
            assert_eq!(report.failed, 1);
            assert_eq!(report.deleted, 0);
            assert_eq!(next, previous);
        }
    }

}
