//! Edits a note's YAML frontmatter one top-level key at a time, leaving the other keys, their
//! order, and their formatting as they were.
//!
//! New values are written as JSON, which YAML reads as flow style: `"text"`, `3`, `true`,
//! `["a", "b"]`. That keeps the writer small and its output valid without a YAML library.

use serde_json::{Map, Value};

/// The frontmatter block at the start of `content`: its inner text and the body after it.
/// `None` when the note has no frontmatter.
pub fn split(content: &str) -> Option<(&str, &str)> {
    let first_end = content.find('\n')?;
    if content[..first_end].trim_end_matches('\r') != "---" {
        return None;
    }
    let inner_start = first_end + 1;
    let mut offset = inner_start;
    for line in content[inner_start..].split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            let inner = &content[inner_start..offset];
            return crate::links::is_property_block(inner).then_some((inner, &content[offset + line.len()..]));
        }
        offset += line.len();
    }
    None
}

/// The top-level key a frontmatter line starts, if it starts one.
pub(crate) fn key_of(line: &str) -> Option<String> {
    if line.starts_with([' ', '\t', '#', '-']) {
        return None;
    }
    if line.starts_with('"') {
        let mut stream = serde_json::Deserializer::from_str(line).into_iter::<String>();
        let key = stream.next()?.ok()?;
        return line[stream.byte_offset()..].trim_start().starts_with(':').then_some(key);
    }
    if let Some(rest) = line.strip_prefix('\'') {
        let mut chars = rest.char_indices().peekable();
        let mut key = String::new();
        while let Some((index, ch)) = chars.next() {
            if ch == '\'' {
                if chars.peek().is_some_and(|(_, next)| *next == '\'') {
                    chars.next();
                    key.push('\'');
                } else {
                    return rest[index + 1..].trim_start().starts_with(':').then_some(key);
                }
            } else {
                key.push(ch);
            }
        }
        return None;
    }
    let (key, _) = line.split_once(':')?;
    let key = key.trim();
    (!key.is_empty()).then(|| key.to_string())
}

fn check_key(key: &str) -> Result<(), String> {
    let valid = !key.trim().is_empty()
        && key.trim() == key
        && !key.contains([':', '\n', '\r', '#'])
        && !key.chars().any(char::is_control)
        && !key.starts_with(['-', '"', '\'', '[', '{', ']', '}', '*', '&', '!', '?', '|', '>', '%', '@', '`']);
    if valid {
        Ok(())
    } else {
        Err(format!("'{key}' is not a usable property name"))
    }
}

/// Sets each key in `set` (a `null` value removes the key) and removes each key in `remove`.
/// A key that exists keeps its place; a new key goes at the end. A note without frontmatter
/// gains a block, and a block left empty is dropped.
pub fn set_properties(content: &str, set: &Map<String, Value>, remove: &[String]) -> Result<String, String> {
    for key in set.keys().chain(remove) {
        check_key(key)?;
    }
    let newline = if content.contains("\r\n") { "\r\n" } else { "\n" };
    let (inner, body) = split(content).unwrap_or(("", content));

    // Each top-level key owns its line and the indented or list lines under it.
    let mut blocks: Vec<(Option<String>, Vec<String>)> = Vec::new();
    for line in inner.lines() {
        match key_of(line) {
            Some(key) => blocks.push((Some(key), vec![line.to_string()])),
            None => match blocks.last_mut() {
                Some((Some(_), lines)) if line.trim().is_empty() || line.starts_with([' ', '\t', '-']) => lines.push(line.to_string()),
                _ => blocks.push((None, vec![line.to_string()])),
            },
        }
    }

    let is_removed = |key: &str| remove.iter().any(|r| r == key) || matches!(set.get(key), Some(Value::Null));
    blocks.retain(|(key, _)| key.as_deref().is_none_or(|k| !is_removed(k)));
    for (key, value) in set {
        if value.is_null() {
            continue;
        }
        let encoded = serde_json::to_string(value).map_err(|e| format!("Cannot encode '{key}': {e}"))?;
        let line = format!("{key}: {encoded}");
        match blocks.iter_mut().find(|(k, _)| k.as_deref() == Some(key.as_str())) {
            Some(block) => block.1 = vec![line],
            None => blocks.push((Some(key.clone()), vec![line])),
        }
    }

    let lines: Vec<String> = blocks.into_iter().flat_map(|(_, lines)| lines).collect();
    if lines.iter().all(|l| l.trim().is_empty()) {
        return Ok(body.to_string());
    }
    let mut out = format!("---{newline}");
    for line in lines {
        out.push_str(&line);
        out.push_str(newline);
    }
    out.push_str("---");
    out.push_str(newline);
    out.push_str(body);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn map(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn split_finds_the_block_and_the_body() {
        assert_eq!(split("---\na: 1\n---\nBody\n"), Some(("a: 1\n", "Body\n")));
        assert_eq!(split("---\r\na: 1\r\n---\r\nBody"), Some(("a: 1\r\n", "Body")));
        assert_eq!(split("No frontmatter\n---\n"), None);
        assert_eq!(split("---\nnever closed\n"), None);
    }

    #[test]
    fn existing_keys_keep_their_place_and_new_keys_go_last() {
        let note = "---\ntitle: Plan\ntags:\n  - work\n# a comment\nstatus: draft\n---\nBody\n";
        let out = set_properties(note, &map(json!({ "status": "done", "due": "2026-12-01" })), &[]).unwrap();
        assert_eq!(out, "---\ntitle: Plan\ntags:\n  - work\n# a comment\nstatus: \"done\"\ndue: \"2026-12-01\"\n---\nBody\n");
    }

    #[test]
    fn a_multiline_value_is_replaced_whole_and_removal_drops_it() {
        let note = "---\ntags:\n  - a\n  - b\nrating: 3\n---\nBody\n";
        let out = set_properties(note, &map(json!({ "tags": ["c"] })), &["rating".to_string()]).unwrap();
        assert_eq!(out, "---\ntags: [\"c\"]\n---\nBody\n");
        let cleared = set_properties(&out, &map(json!({ "tags": null })), &[]).unwrap();
        assert_eq!(cleared, "Body\n");
    }

    #[test]
    fn a_note_without_frontmatter_gains_one_and_keeps_crlf() {
        assert_eq!(set_properties("Body\n", &map(json!({ "done": true })), &[]).unwrap(), "---\ndone: true\n---\nBody\n");
        assert_eq!(set_properties("A\r\nB\r\n", &map(json!({ "n": 2 })), &[]).unwrap(), "---\r\nn: 2\r\n---\r\nA\r\nB\r\n");
    }

    #[test]
    fn unusable_keys_are_refused() {
        for key in ["", "a:b", "- x", "# c", " padded", "*oops", "&anchor", "!tag", "? key", "|", ">", "%directive", "@key", "`key", "bad\tkey"] {
            assert!(set_properties("Body", &map(json!({ key: 1 })), &[]).is_err(), "{key:?}");
        }
    }

    #[test]
    fn blank_lines_stay_with_multiline_values_during_replacement_and_removal() {
        for marker in ["|", ">", "|-"] {
            let note = format!("---\ndescription: {marker}\n  first\n\n  second\nstatus: draft\n---\nBody\n");
            assert_eq!(set_properties(&note, &map(json!({"description": "done"})), &[]).unwrap(),
                "---\ndescription: \"done\"\nstatus: draft\n---\nBody\n");
            assert_eq!(set_properties(&note, &Map::new(), &["description".into()]).unwrap(),
                "---\nstatus: draft\n---\nBody\n");
            assert_eq!(set_properties(&note, &Map::new(), &[]).unwrap(), note);
        }
    }

    #[test]
    fn quoted_keys_are_updated_and_removed_by_their_decoded_name() {
        for key in ["\"status\"", "'status'", "\"sta\\u0074us\""] {
            let note = format!("---\n{key}: draft\n---\nBody\n");
            assert_eq!(set_properties(&note, &map(json!({"status": "done"})), &[]).unwrap(),
                "---\nstatus: \"done\"\n---\nBody\n");
            assert_eq!(set_properties(&note, &Map::new(), &["status".into()]).unwrap(), "Body\n");
        }
        assert_eq!(key_of("'owner''s status': draft"), Some("owner's status".into()));
    }

    #[test]
    fn thematic_breaks_stay_in_the_body_when_properties_are_added() {
        let note = "---\nIntro text\n---\nBody\n";
        assert_eq!(split(note), None);
        assert_eq!(set_properties(note, &map(json!({"status": "done"})), &[]).unwrap(),
            format!("---\nstatus: \"done\"\n---\n{note}"));
    }
}
