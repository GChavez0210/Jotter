//! A Model Context Protocol server over standard input and output.
//!
//! One JSON-RPC message per line. The server offers the same operations as the command line
//! as tools. A read-only server lists, and runs, only the tools that do not change the vault.

use std::io::{BufRead, Write};

use jotter_core::agent::AgentVault;
use serde_json::{json, Value};

const DEFAULT_PROTOCOL_VERSION: &str = "2025-06-18";

/// Property name, JSON type, description, required.
type Param = (&'static str, &'static str, &'static str, bool);

struct Tool {
    name: &'static str,
    description: &'static str,
    writes: bool,
    params: &'static [Param],
}

const PATH: Param = ("path", "string", "Note path inside the vault, with or without .md.", true);

const TOOLS: &[Tool] = &[
    Tool { name: "search_notes", description: "Full-text search over the vault. Returns matching notes with snippets.", writes: false,
        params: &[("query", "string", "Words to search for.", true), ("limit", "integer", "Most results to return. Default 20.", false)] },
    Tool { name: "list_notes", description: "List notes with their metadata and link counts.", writes: false,
        params: &[("folder", "string", "Only notes under this folder.", false)] },
    Tool { name: "read_note", description: "Read a note's Markdown.", writes: false, params: &[PATH] },
    Tool { name: "create_note", description: "Create a note. Fails if it already exists.", writes: true,
        params: &[PATH, ("content", "string", "Markdown for the note.", true)] },
    Tool { name: "append_to_note", description: "Add text to the end of a note. The previous text stays in version history.", writes: true,
        params: &[PATH, ("text", "string", "Text to add.", true), ("create", "boolean", "Create the note if it does not exist.", false)] },
    Tool { name: "rename_note", description: "Rename or move a note or folder, rewriting links that point to it.", writes: true,
        params: &[("from", "string", "Current path.", true), ("to", "string", "New path.", true), ("update_links", "boolean", "Rewrite links to the note. Default true.", false)] },
    Tool { name: "delete_note", description: "Move a note or folder to the Recycle Bin.", writes: true, params: &[PATH] },
    Tool { name: "get_backlinks", description: "Notes that link to a note.", writes: false, params: &[PATH] },
    Tool { name: "get_tags", description: "All tags with note counts, or the notes that carry one tag.", writes: false,
        params: &[("tag", "string", "A tag, without #, to list its notes.", false)] },
    Tool { name: "get_tasks", description: "Checklist items that have a due date.", writes: false, params: &[] },
    Tool { name: "vault_health", description: "Broken links, unused attachments, empty notes, and duplicate titles.", writes: false, params: &[] },
    Tool { name: "list_versions", description: "Stored versions of a note, newest first.", writes: false, params: &[PATH] },
    Tool { name: "read_version", description: "Read one stored version of a note.", writes: false,
        params: &[PATH, ("id", "string", "Version id from list_versions.", true)] },
];

fn schema(tool: &Tool) -> Value {
    let properties: serde_json::Map<String, Value> = tool
        .params
        .iter()
        .map(|(name, kind, description, _)| (name.to_string(), json!({ "type": kind, "description": description })))
        .collect();
    let required: Vec<&str> = tool.params.iter().filter(|p| p.3).map(|p| p.0).collect();
    json!({ "type": "object", "properties": properties, "required": required })
}

fn text<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key).and_then(Value::as_str).ok_or_else(|| format!("Missing the text argument '{key}'"))
}

fn flag(args: &Value, key: &str, default: bool) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(default)
}

pub fn call_tool(agent: &AgentVault, read_only: bool, name: &str, args: &Value) -> Result<Value, String> {
    let tool = TOOLS.iter().find(|t| t.name == name).ok_or_else(|| format!("Unknown tool '{name}'"))?;
    if tool.writes && read_only {
        return Err("This server is read-only, so writes are refused".to_string());
    }
    match name {
        "search_notes" => agent.search(text(args, "query")?, args.get("limit").and_then(Value::as_u64).unwrap_or(20) as usize),
        "list_notes" => agent.list(args.get("folder").and_then(Value::as_str)),
        "read_note" => agent.read(text(args, "path")?),
        "create_note" => agent.create(text(args, "path")?, text(args, "content")?),
        "append_to_note" => agent.append(text(args, "path")?, text(args, "text")?, flag(args, "create", false)),
        "rename_note" => agent.rename(text(args, "from")?, text(args, "to")?, flag(args, "update_links", true)),
        "delete_note" => agent.delete(text(args, "path")?),
        "get_backlinks" => agent.backlinks(text(args, "path")?),
        "get_tags" => agent.tags(args.get("tag").and_then(Value::as_str)),
        "get_tasks" => agent.tasks(),
        "vault_health" => agent.health(),
        "list_versions" => agent.versions(text(args, "path")?),
        "read_version" => agent.version(text(args, "path")?, text(args, "id")?),
        _ => Err(format!("Unknown tool '{name}'")),
    }
}

/// Answers one JSON-RPC message. Notifications get no answer.
pub fn handle(agent: &AgentVault, read_only: bool, message: &Value) -> Option<Value> {
    let id = message.get("id")?.clone();
    let method = message.get("method").and_then(Value::as_str).unwrap_or_default();
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let outcome: Result<Value, (i64, String)> = match method {
        "initialize" => Ok(json!({
            "protocolVersion": params.get("protocolVersion").and_then(Value::as_str).unwrap_or(DEFAULT_PROTOCOL_VERSION),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "jotter", "version": env!("CARGO_PKG_VERSION") },
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({
            "tools": TOOLS.iter().filter(|t| !(read_only && t.writes)).map(|t| json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": schema(t),
            })).collect::<Vec<_>>(),
        })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            // A failed tool is a result the model can read, not a protocol error.
            Ok(match call_tool(agent, read_only, name, &args) {
                Ok(value) => json!({ "content": [{ "type": "text", "text": value.to_string() }], "isError": false }),
                Err(message) => json!({ "content": [{ "type": "text", "text": message }], "isError": true }),
            })
        }
        other => Err((-32601, format!("Method not found: {other}"))),
    };
    Some(match outcome {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }),
    })
}

/// Reads messages from standard input until it closes.
pub fn serve(agent: &AgentVault, read_only: bool) -> Result<(), String> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line.map_err(|e| format!("Cannot read standard input: {e}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(message) => handle(agent, read_only, &message),
            Err(e) => Some(json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": format!("Parse error: {e}") } })),
        };
        if let Some(reply) = reply {
            writeln!(stdout, "{reply}").and_then(|_| stdout.flush()).map_err(|e| format!("Cannot write the answer: {e}"))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(read_only: bool) -> (tempfile::TempDir, tempfile::TempDir, AgentVault) {
        let vault = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        std::fs::write(vault.path().join("Plan.md"), "oranges").unwrap();
        let agent = AgentVault::open(vault.path(), read_only, data.path()).unwrap();
        (vault, data, agent)
    }

    fn rpc(agent: &AgentVault, read_only: bool, method: &str, params: Value) -> Value {
        handle(agent, read_only, &json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params })).unwrap()
    }

    #[test]
    fn initialize_echoes_the_clients_protocol_version() {
        let (_v, _d, agent) = agent(false);
        let reply = rpc(&agent, false, "initialize", json!({ "protocolVersion": "2025-03-26" }));
        assert_eq!(reply["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(reply["result"]["serverInfo"]["name"], "jotter");
    }

    #[test]
    fn notifications_get_no_answer() {
        let (_v, _d, agent) = agent(false);
        assert!(handle(&agent, false, &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).is_none());
    }

    #[test]
    fn a_read_only_server_lists_and_runs_only_read_tools() {
        let (_v, _d, agent) = agent(true);
        let names: Vec<String> = rpc(&agent, true, "tools/list", json!({}))["result"]["tools"]
            .as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect();
        assert!(names.contains(&"read_note".to_string()));
        assert!(!names.contains(&"append_to_note".to_string()));
        let reply = rpc(&agent, true, "tools/call", json!({ "name": "delete_note", "arguments": { "path": "Plan" } }));
        assert_eq!(reply["result"]["isError"], true);
    }

    #[test]
    fn a_tool_call_returns_json_text_and_a_failure_is_a_readable_result() {
        let (_v, _d, agent) = agent(false);
        let ok = rpc(&agent, false, "tools/call", json!({ "name": "read_note", "arguments": { "path": "Plan" } }));
        assert_eq!(ok["result"]["isError"], false);
        let body: Value = serde_json::from_str(ok["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(body["content"], "oranges");
        let missing = rpc(&agent, false, "tools/call", json!({ "name": "read_note", "arguments": { "path": "Nope" } }));
        assert_eq!(missing["result"]["isError"], true);
        let bad = rpc(&agent, false, "tools/call", json!({ "name": "read_note", "arguments": {} }));
        assert!(bad["result"]["content"][0]["text"].as_str().unwrap().contains("'path'"));
    }

    #[test]
    fn unknown_methods_are_protocol_errors() {
        let (_v, _d, agent) = agent(false);
        assert_eq!(rpc(&agent, false, "resources/list", json!({}))["error"]["code"], -32601);
    }
}
