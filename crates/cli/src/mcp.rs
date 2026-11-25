//! `autoupload mcp`: a Model Context Protocol server on stdin/stdout, so AI agents (Claude Code, Codex, Cursor…)
//! can drive AutoUpload with typed tools instead of parsing terminal output.
//!
//! JSON-RPC 2.0, one message per line. stdout carries only protocol messages; diagnostics go to stderr.
//! Every tool call is recorded in the activity log under the agent's name (from `clientInfo`), and the app's
//! Activity tab shows it. Uploading needs the person's permission (see [`autoupload_core::ops`]).

use autoupload_core::activity::Actor;
use autoupload_core::ops::{self, SheetFields};
use autoupload_core::paths::data_root;
use serde_json::{json, Map, Value};
use std::io::{BufRead, Write};
use std::path::PathBuf;

const PROTOCOL: &str = "2025-06-18";
const SUPPORTED: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "AutoUpload uploads videos to the user's own YouTube channels as private videos with a scheduled publish time. \
Typical flow: status → add_video for each new video (title required; write a good description and tags) → list_queue to check nothing is blocked → plan to preview the dates → upload only if status says agents_may_upload is true; otherwise show the plan and ask the user to press Upload in the AutoUpload app. \
Never invent facts about a video: ask the user for what you cannot see. Nothing you do is ever published as public at once.";

fn sheet_props() -> Map<String, Value> {
    let mut m = Map::new();
    m.insert(
        "title".into(),
        json!({"type": "string", "description": "Video title, at most 100 characters"}),
    );
    m.insert("description".into(), json!({"type": "string"}));
    m.insert(
        "tags".into(),
        json!({"type": "array", "items": {"type": "string"}}),
    );
    m.insert("publish_at".into(), json!({"type": "string", "description": "Fixed publish time, ISO 8601 with offset (2026-12-01T18:00:00+01:00). Omit to take the channel's next free slot."}));
    m.insert("playlist_id".into(), json!({"type": "string"}));
    m.insert("category_id".into(), json!({"type": "string", "description": "YouTube category ID, e.g. 27 Education, 28 Science & Technology"}));
    m.insert(
        "default_language".into(),
        json!({"type": "string", "description": "BCP-47, e.g. en, es"}),
    );
    m.insert("made_for_kids".into(), json!({"type": "boolean"}));
    m.insert("contains_synthetic_media".into(), json!({"type": "boolean", "description": "True if the video has realistic AI-generated or altered voice or imagery"}));
    m.insert("notify_subscribers".into(), json!({"type": "boolean"}));
    m
}

fn schema(extra: &[(&str, Value)], with_sheet: bool, required: &[&str]) -> Value {
    let mut props = if with_sheet {
        sheet_props()
    } else {
        Map::new()
    };
    for (k, v) in extra {
        props.insert((*k).into(), v.clone());
    }
    json!({"type": "object", "properties": props, "required": required, "additionalProperties": false})
}

fn tool(
    name: &str,
    title: &str,
    description: &str,
    input: Value,
    read_only: bool,
    destructive: bool,
) -> Value {
    json!({
        "name": name,
        "title": title,
        "description": description,
        "inputSchema": input,
        "annotations": {"readOnlyHint": read_only, "destructiveHint": destructive, "idempotentHint": read_only, "openWorldHint": !read_only},
    })
}

fn tools() -> Value {
    let channel = (
        "channel",
        json!({"type": "string", "description": "Channel name as shown by status"}),
    );
    let max = (
        "max",
        json!({"type": "integer", "minimum": 1, "description": "At most this many videos"}),
    );
    json!([
        tool("status", "Status", "Data folder, channels (signed in or not, schedule, queued videos) and whether agents may upload.", schema(&[], false, &[]), true, false),
        tool("list_queue", "List queue", "Videos waiting in a channel's queue, with the problem that blocks each one, if any.", schema(std::slice::from_ref(&channel), false, &["channel"]), true, false),
        tool("add_video", "Add video", "Copy (or move) a video file into a channel's queue and write its details file. The video then waits for plan/upload.",
            schema(&[channel.clone(),
                ("video", json!({"type": "string", "description": "Absolute path of the video file"})),
                ("thumbnail", json!({"type": "string", "description": "Absolute path of a .jpg or .png thumbnail"})),
                ("move", json!({"type": "boolean", "description": "Move instead of copy (default false)"}))], true, &["channel", "video", "title"]), false, false),
        tool("write_sheet", "Write details", "Write or replace the details file (title, description, tags…) of a video already in the queue.",
            schema(&[channel.clone(), ("file", json!({"type": "string", "description": "File name in the queue, e.g. episode-4.mp4"}))], true, &["channel", "file", "title"]), false, true),
        tool("plan", "Preview dates", "Read the channel's YouTube calendar and show the date each queued video would get. Uploads nothing.", schema(&[channel.clone(), max.clone()], false, &["channel"]), true, false),
        tool("upload", "Upload", "Upload the queue as private videos with their scheduled dates. Refused unless the person allowed agents to upload.", schema(&[channel.clone(), max], false, &["channel"]), false, false),
        tool("calendar", "Calendar", "Scheduled and recent uploads of a channel, in its time zone.", schema(&[channel, ("days", json!({"type": "integer", "minimum": 1, "maximum": 365}))], false, &["channel"]), true, false),
        tool("doctor", "Diagnostics", "Offline checks of the setup with the fix for each problem.", schema(&[], false, &[]), true, false),
        tool("activity", "Activity", "Recent actions by the person, the daily task and agents.", schema(&[("limit", json!({"type": "integer", "minimum": 1, "maximum": 500}))], false, &[]), true, false),
    ])
}

fn s<'a>(a: &'a Value, k: &str) -> Result<&'a str, String> {
    a.get(k)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing '{k}'"))
}

fn n(a: &Value, k: &str) -> Option<usize> {
    a.get(k).and_then(Value::as_u64).map(|v| v as usize)
}

fn sheet(a: &Value) -> Result<SheetFields, String> {
    let mut m = Map::new();
    for k in sheet_props().keys() {
        if let Some(v) = a.get(k) {
            m.insert(k.clone(), v.clone());
        }
    }
    serde_json::from_value(Value::Object(m)).map_err(|e| format!("invalid details: {e}"))
}

fn call(actor: &Actor, name: &str, a: &Value) -> Result<Value, String> {
    let home = data_root().0;
    match name {
        "status" => Ok(ops::status(&home)),
        "list_queue" => ops::queue(&home, actor, s(a, "channel")?),
        "add_video" => {
            let thumb = a
                .get("thumbnail")
                .and_then(Value::as_str)
                .map(PathBuf::from);
            ops::add_video(
                &home,
                actor,
                s(a, "channel")?,
                &PathBuf::from(s(a, "video")?),
                &sheet(a)?,
                a.get("move").and_then(Value::as_bool).unwrap_or(false),
                thumb.as_deref(),
            )
        }
        "write_sheet" => {
            ops::write_sheet(&home, actor, s(a, "channel")?, s(a, "file")?, &sheet(a)?)
        }
        "plan" => ops::run(&home, actor, s(a, "channel")?, false, n(a, "max")),
        "upload" => ops::run(&home, actor, s(a, "channel")?, true, n(a, "max")),
        "calendar" => ops::calendar(
            &home,
            actor,
            s(a, "channel")?,
            n(a, "days").unwrap_or(30) as i64,
        ),
        "doctor" => Ok(ops::doctor_json()),
        "activity" => Ok(ops::activity_json(&home, n(a, "limit").unwrap_or(50))),
        _ => Err(format!("unknown tool '{name}'")),
    }
}

fn reply(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// Handles one message; `None` for notifications, which get no answer.
pub fn handle(actor: &mut Actor, msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let id = id?; // notifications (notifications/initialized, cancelled…) need no reply
    Some(match method {
        "initialize" => {
            if let Some(name) = params["clientInfo"]["name"].as_str() {
                *actor = Actor::agent(name);
            }
            let asked = params["protocolVersion"].as_str().unwrap_or(PROTOCOL);
            let version = if SUPPORTED.contains(&asked) {
                asked
            } else {
                PROTOCOL
            };
            reply(
                &id,
                json!({
                    "protocolVersion": version,
                    "capabilities": {"tools": {"listChanged": false}},
                    "serverInfo": {"name": "autoupload", "title": "AutoUpload", "version": autoupload_core::VERSION},
                    "instructions": INSTRUCTIONS,
                }),
            )
        }
        "ping" => reply(&id, json!({})),
        "tools/list" => reply(&id, json!({"tools": tools()})),
        "tools/call" => {
            let name = params["name"].as_str().unwrap_or("");
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let (value, is_error) = match call(actor, name, &args) {
                Ok(v) => (v, false),
                Err(e) => (json!({"error": e}), true),
            };
            let text = serde_json::to_string_pretty(&value).unwrap_or_default();
            reply(
                &id,
                json!({"content": [{"type": "text", "text": text}], "structuredContent": value, "isError": is_error}),
            )
        }
        _ => error(&id, -32601, &format!("method not found: {method}")),
    })
}

pub fn serve() -> i32 {
    let mut actor = Actor::agent("mcp-client");
    eprintln!(
        "AutoUpload {} MCP server on stdio. Data folder: {}",
        autoupload_core::VERSION,
        data_root().0.display()
    );
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let answer = match serde_json::from_str::<Value>(&line) {
            Ok(Value::Array(batch)) => {
                let all: Vec<Value> = batch.iter().filter_map(|m| handle(&mut actor, m)).collect();
                (!all.is_empty()).then_some(Value::Array(all))
            }
            Ok(msg) => handle(&mut actor, &msg),
            Err(e) => Some(error(&Value::Null, -32700, &format!("parse error: {e}"))),
        };
        if let Some(a) = answer {
            if writeln!(out, "{a}").and_then(|_| out.flush()).is_err() {
                break;
            }
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handshake_and_tool_list() {
        let mut actor = Actor::agent("x");
        let init = handle(
            &mut actor,
            &json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "clientInfo": {"name": "claude-code", "version": "1"}, "capabilities": {}}}),
        )
        .unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(actor, Actor::agent("claude-code"));
        assert!(handle(
            &mut actor,
            &json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
        )
        .is_none());
        let list = handle(
            &mut actor,
            &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        )
        .unwrap();
        let names: Vec<&str> = list["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"add_video") && names.contains(&"upload"));
        let unknown = handle(
            &mut actor,
            &json!({"jsonrpc": "2.0", "id": 3, "method": "nope"}),
        )
        .unwrap();
        assert_eq!(unknown["error"]["code"], -32601);
        let bad = handle(&mut actor, &json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "list_queue", "arguments": {}}})).unwrap();
        assert_eq!(bad["result"]["isError"], true);
    }

    #[test]
    fn sheet_fields_from_arguments() {
        let f =
            sheet(&json!({"channel": "c", "title": "T", "tags": ["a"], "made_for_kids": false}))
                .unwrap();
        assert_eq!(f.title.as_deref(), Some("T"));
        assert_eq!(f.tags.unwrap(), vec!["a"]);
    }
}
