//! The shared activity log: one JSON line per action in `<data folder>/activity.jsonl`.
//!
//! The desktop app, the command line, the daily task and AI agents (through `autoupload mcp` or with
//! `AUTOUPLOAD_AGENT` set) all append to it, so the app's Activity tab shows who did what, and when.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Who acted.
pub const APP: &str = "app";
pub const CLI: &str = "cli";
pub const AGENT: &str = "agent";
pub const TASK: &str = "task";

/// The log is rotated to `activity.1.jsonl` past this size.
const MAX_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub time: DateTime<Utc>,
    /// `app`, `cli`, `agent` or `task`.
    pub actor: String,
    /// The AI agent's name (MCP client name or `AUTOUPLOAD_AGENT`), when the actor is an agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    /// Short machine-friendly verb: `add_video`, `plan`, `upload`, `calendar`, `connect`, `run`…
    pub action: String,
    /// One human sentence.
    pub summary: String,
    pub ok: bool,
}

/// Who is running this process: an agent if `AUTOUPLOAD_AGENT` names one, otherwise the given default.
#[derive(Debug, Clone, PartialEq)]
pub struct Actor {
    pub kind: String,
    pub agent: Option<String>,
}

impl Actor {
    pub fn new(kind: &str) -> Actor {
        Actor {
            kind: kind.into(),
            agent: None,
        }
    }

    pub fn agent(name: &str) -> Actor {
        Actor {
            kind: AGENT.into(),
            agent: Some(name.to_string()),
        }
    }

    /// `AUTOUPLOAD_AGENT=<name>` marks a terminal session driven by an AI agent.
    pub fn from_env(default: &str) -> Actor {
        match std::env::var("AUTOUPLOAD_AGENT") {
            Ok(n) if !n.trim().is_empty() => Actor::agent(n.trim()),
            _ => Actor::new(default),
        }
    }

    pub fn is_agent(&self) -> bool {
        self.kind == AGENT
    }

    /// Appends one entry; a failure to write the log never stops the real work.
    pub fn record(
        &self,
        home: &Path,
        channel: Option<&str>,
        action: &str,
        summary: impl Into<String>,
        ok: bool,
    ) {
        let _ = append(
            home,
            &Entry {
                time: Utc::now(),
                actor: self.kind.clone(),
                agent: self.agent.clone(),
                channel: channel.map(String::from),
                action: action.into(),
                summary: summary.into(),
                ok,
            },
        );
    }
}

pub fn log_path(home: &Path) -> PathBuf {
    home.join("activity.jsonl")
}

pub fn append(home: &Path, entry: &Entry) -> std::io::Result<()> {
    std::fs::create_dir_all(home)?;
    let path = log_path(home);
    if std::fs::metadata(&path)
        .map(|m| m.len() > MAX_BYTES)
        .unwrap_or(false)
    {
        let _ = std::fs::rename(&path, home.join("activity.1.jsonl"));
    }
    let line = serde_json::to_string(entry).map_err(std::io::Error::other)?;
    let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
    writeln!(f, "{line}")
}

/// The last `limit` entries, oldest first. Lines that do not parse are skipped.
pub fn recent(home: &Path, limit: usize) -> Vec<Entry> {
    let Ok(text) = std::fs::read_to_string(log_path(home)) else {
        return vec![];
    };
    let mut all: Vec<Entry> = text
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    if all.len() > limit {
        all.drain(..all.len() - limit);
    }
    all
}

/// "AI · claude-code", "You · app", "Command line", "Daily task".
pub fn who(e: &Entry) -> String {
    match e.actor.as_str() {
        AGENT => format!("AI · {}", e.agent.as_deref().unwrap_or("agent")),
        APP => "You · app".into(),
        CLI => "Command line".into(),
        TASK => "Daily task".into(),
        other => other.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_and_read_back() {
        let d = tempfile::tempdir().unwrap();
        Actor::agent("claude-code").record(
            d.path(),
            Some("demo"),
            "add_video",
            "Queued a.mp4",
            true,
        );
        Actor::new(APP).record(d.path(), None, "connect", "Connected", false);
        std::fs::write(
            log_path(d.path()),
            std::fs::read_to_string(log_path(d.path())).unwrap() + "not json\n",
        )
        .unwrap();
        let all = recent(d.path(), 10);
        assert_eq!(all.len(), 2);
        assert_eq!(who(&all[0]), "AI · claude-code");
        assert_eq!(all[0].channel.as_deref(), Some("demo"));
        assert!(!all[1].ok);
        assert_eq!(recent(d.path(), 1)[0].action, "connect");
    }

    #[test]
    fn actor_from_env_defaults() {
        // Only the default branch is checked: changing the environment would race with other tests.
        if std::env::var("AUTOUPLOAD_AGENT").is_err() {
            assert_eq!(Actor::from_env(CLI), Actor::new(CLI));
        }
    }
}
