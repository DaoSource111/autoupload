//! The operations an AI agent (or a script) can run, with JSON results. `autoupload mcp` exposes them as MCP
//! tools and the command line prints them with `--json`. Every call is written to the activity log.
//!
//! Safety rules enforced here, whoever the caller is:
//! - an agent can always read, queue videos, write details files and preview dates;
//! - an agent can upload only if a person turned on "AI agents may upload" (`agents_may_upload` in settings.json);
//! - nothing started from here is ever made public at once: sheets with `privacy: public` are refused.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

use crate::activity::{self, Actor};
use crate::auth;
use crate::channel::{list_channels, load_channel, Channel};
use crate::client::api_for;
use crate::demo;
use crate::doctor::{self, Level};
use crate::manifest::parse_publish_at;
use crate::paths::Settings;
use crate::queue::{build_manifest, is_settling, queue_items, SETTLE};
use crate::runner::{calendar_rows, process_queue, Event, RunOptions};
use crate::validate::{ext_lower, THUMB_EXT, VIDEO_EXT};

/// The details a sheet can hold that an agent may set. Field names are the sheet keys.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SheetFields {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// ISO 8601 with offset; without it the video takes the next free slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publish_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playlist_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub made_for_kids: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contains_synthetic_media: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notify_subscribers: Option<bool>,
}

impl SheetFields {
    fn check(&self) -> Result<(), String> {
        match self.title.as_deref().map(str::trim) {
            None | Some("") => return Err("a title is required".into()),
            Some(t) if t.chars().count() > 100 => {
                return Err("the title is longer than YouTube's 100 characters".into())
            }
            _ => {}
        }
        if let Some(p) = &self.publish_at {
            parse_publish_at(p)?;
        }
        Ok(())
    }

    fn yaml(&self) -> Result<String, String> {
        serde_yaml::to_string(self).map_err(|e| e.to_string())
    }
}

fn channel(home: &Path, name: &str) -> Result<Channel, String> {
    load_channel(name, home)
}

fn levels(l: &Level) -> &'static str {
    match l {
        Level::Ok => "ok",
        Level::Warning => "warning",
        Level::Error => "error",
    }
}

/// Data folder, channels and what agents are allowed to do.
pub fn status(home: &Path) -> Value {
    let channels: Vec<Value> = list_channels(home)
        .into_iter()
        .map(|n| match load_channel(&n, home) {
            Ok(ch) => json!({
                "name": n,
                "connected": auth::read_token(&ch.profile).is_some(),
                "youtube_channel_id": ch.expected_channel_id(),
                "schedule": ch.schedule_text(),
                "queued": queue_items(&ch).len(),
            }),
            Err(e) => json!({"name": n, "error": e}),
        })
        .collect();
    json!({
        "version": crate::VERSION,
        "data_folder": home,
        "demo": demo::active(),
        "agents_may_upload": Settings::load().agents_may_upload,
        "channels": channels,
    })
}

/// The videos waiting in a channel's queue, each with what (if anything) stops it from being uploaded.
pub fn queue(home: &Path, actor: &Actor, name: &str) -> Result<Value, String> {
    let ch = channel(home, name)?;
    let mut ready = 0;
    let items: Vec<Value> = queue_items(&ch)
        .iter()
        .map(|it| {
            let settling = is_settling(&it.video, SETTLE);
            let (title, problem) = match build_manifest(&ch, it, None) {
                Ok(m) => (Some(m.title), None),
                Err(e) => (None, Some(e)),
            };
            if problem.is_none() && !settling {
                ready += 1;
            }
            json!({
                "file": it.file_name(),
                "title": title,
                "has_sheet": it.sheet.is_some(),
                "has_thumbnail": it.thumbnail.is_some(),
                "still_copying": settling,
                "problem": problem,
            })
        })
        .collect();
    let blocked = items.len() - ready;
    actor.record(
        home,
        Some(name),
        "queue",
        format!("Read the queue: {ready} ready, {blocked} not ready"),
        true,
    );
    Ok(json!({"channel": name, "ready": ready, "items": items}))
}

/// Copies (or moves) a video into a channel's queue and writes its details file.
pub fn add_video(
    home: &Path,
    actor: &Actor,
    name: &str,
    video: &Path,
    fields: &SheetFields,
    move_file: bool,
    thumbnail: Option<&Path>,
) -> Result<Value, String> {
    let res = add_video_inner(home, name, video, fields, move_file, thumbnail);
    let file = video
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    match &res {
        Ok(_) => {
            let tags = fields.tags.as_ref().map(|t| t.len()).unwrap_or(0);
            actor.record(
                home,
                Some(name),
                "add_video",
                format!(
                    "Queued {file}: \"{}\"{}",
                    fields.title.as_deref().unwrap_or(""),
                    if tags > 0 {
                        format!(", {tags} tags")
                    } else {
                        String::new()
                    }
                ),
                true,
            )
        }
        Err(e) => actor.record(
            home,
            Some(name),
            "add_video",
            format!("Could not queue {file}: {e}"),
            false,
        ),
    }
    res
}

fn add_video_inner(
    home: &Path,
    name: &str,
    video: &Path,
    fields: &SheetFields,
    move_file: bool,
    thumbnail: Option<&Path>,
) -> Result<Value, String> {
    let ch = channel(home, name)?;
    fields.check()?;
    if !video.is_file() {
        return Err(format!("{} is not a file", video.display()));
    }
    if !VIDEO_EXT.contains(&ext_lower(video).as_str()) {
        return Err(format!(
            "not a video file (allowed: {})",
            VIDEO_EXT.join(", ")
        ));
    }
    if let Some(t) = thumbnail {
        if !t.is_file() || !THUMB_EXT.contains(&ext_lower(t).as_str()) {
            return Err(format!(
                "thumbnail must be an existing {} file",
                THUMB_EXT.join("/")
            ));
        }
    }
    let file_name = video.file_name().ok_or("the video has no file name")?;
    let dest = ch.queue().join(file_name);
    let sheet = dest.with_extension("yaml");
    if dest.exists() || sheet.exists() {
        return Err(format!("{} is already in the queue", dest.display()));
    }
    std::fs::create_dir_all(ch.queue()).map_err(|e| e.to_string())?;
    // The sheet goes first: a video without its sheet would be reported as invalid by a run starting meanwhile.
    std::fs::write(&sheet, fields.yaml()?).map_err(|e| e.to_string())?;
    let placed = if move_file {
        std::fs::rename(video, &dest)
            .or_else(|_| std::fs::copy(video, &dest).and_then(|_| std::fs::remove_file(video)))
    } else {
        std::fs::copy(video, &dest).map(|_| ())
    };
    if let Err(e) = placed {
        let _ = std::fs::remove_file(&sheet);
        return Err(format!("could not place the video in the queue: {e}"));
    }
    if let Some(t) = thumbnail {
        let to = dest.with_extension(ext_lower(t));
        std::fs::copy(t, &to).map_err(|e| format!("could not copy the thumbnail: {e}"))?;
    }
    Ok(json!({
        "channel": name,
        "queued": dest,
        "sheet": sheet,
        "note": "Copied files keep a fresh modification time, so the video waits about 2 minutes before a run takes it.",
    }))
}

/// Writes (or replaces) the details file of a video that is already in the queue.
pub fn write_sheet(
    home: &Path,
    actor: &Actor,
    name: &str,
    file: &str,
    fields: &SheetFields,
) -> Result<Value, String> {
    let res = (|| {
        let ch = channel(home, name)?;
        fields.check()?;
        let item = queue_items(&ch)
            .into_iter()
            .find(|i| i.file_name() == file)
            .ok_or_else(|| format!("{file} is not in the queue of {name}"))?;
        let sheet = item
            .sheet
            .clone()
            .unwrap_or_else(|| item.video.with_extension("yaml"));
        std::fs::write(&sheet, fields.yaml()?).map_err(|e| e.to_string())?;
        build_manifest(&ch, &item, None)?;
        Ok(json!({"channel": name, "file": file, "sheet": sheet}))
    })();
    actor.record(
        home,
        Some(name),
        "write_sheet",
        match &res {
            Ok(_) => format!(
                "Wrote the details of {file}: \"{}\"",
                fields.title.as_deref().unwrap_or("")
            ),
            Err(e) => format!("Could not write the details of {file}: {e}"),
        },
        res.is_ok(),
    );
    res
}

/// Previews (`execute = false`) or uploads the queue. Agents need the person's permission to upload.
pub fn run(
    home: &Path,
    actor: &Actor,
    name: &str,
    execute: bool,
    max: Option<usize>,
) -> Result<Value, String> {
    let action = if execute { "upload" } else { "plan" };
    if execute && actor.is_agent() && !Settings::load().agents_may_upload {
        let msg = "AI agents may not upload until a person allows it: in the app, AI agents > \"Agents may upload\", or `autoupload agents allow-upload on`. Preview the dates with plan and ask the person to upload.";
        actor.record(
            home,
            Some(name),
            action,
            format!("Upload refused: {msg}"),
            false,
        );
        return Err(msg.into());
    }
    let ch = channel(home, name)?;
    let api = api_for(&ch).inspect_err(|e| {
        actor.record(
            home,
            Some(name),
            action,
            format!("Not signed in: {e}"),
            false,
        );
    })?;
    let opts = RunOptions {
        execute,
        limit: max,
        allow_public: false,
        ..Default::default()
    };
    let (mut planned, mut done, mut warnings, mut errors, mut info) =
        (vec![], vec![], vec![], vec![], vec![]);
    let res = process_queue(&ch, &*api, &opts, &mut |e| match e {
        Event::Planned { file, title, when } => {
            planned.push(json!({"file": file, "title": title, "when": when}))
        }
        Event::Done {
            file,
            url,
            when,
            verified,
        } => done.push(json!({"file": file, "url": url, "when": when, "verified": verified})),
        Event::Warning(s) => warnings.push(s),
        Event::Error(s) => errors.push(s),
        Event::Info(s) => info.push(s),
        Event::Uploading { .. } | Event::Progress { .. } => {}
    });
    match res {
        Ok(_) => {
            let summary = if execute {
                format!(
                    "Uploaded {} video(s){}",
                    done.len(),
                    if errors.is_empty() {
                        String::new()
                    } else {
                        format!(", {} error(s)", errors.len())
                    }
                )
            } else {
                format!("Previewed {} date(s)", planned.len())
            };
            actor.record(home, Some(name), action, summary, errors.is_empty());
            Ok(json!({
                "channel": name,
                "uploaded": execute,
                "planned": planned,
                "done": done,
                "warnings": warnings,
                "errors": errors,
                "info": info,
            }))
        }
        Err(stop) => {
            actor.record(home, Some(name), action, format!("Stopped: {stop}"), false);
            Err(stop.message)
        }
    }
}

/// Scheduled and recent uploads of a channel, in the channel's time zone.
pub fn calendar(home: &Path, actor: &Actor, name: &str, days: i64) -> Result<Value, String> {
    let ch = channel(home, name)?;
    let api = api_for(&ch)?;
    let rows = calendar_rows(&*api, days, Utc::now()).map_err(|e| e.to_string())?;
    let items: Vec<Value> = rows
        .iter()
        .map(|(when, privacy, title)| {
            json!({
                "when": ch.zone.local(*when).format("%Y-%m-%d %H:%M").to_string(),
                "status": if privacy == "private" { "scheduled" } else { privacy.as_str() },
                "title": title,
            })
        })
        .collect();
    actor.record(
        home,
        Some(name),
        "calendar",
        format!("Read the calendar: {} upload(s)", items.len()),
        true,
    );
    Ok(json!({"channel": name, "timezone": ch.cfg.timezone, "uploads": items}))
}

pub fn doctor_json() -> Value {
    let checks: Vec<Value> = doctor::run()
        .into_iter()
        .map(|c| json!({"level": levels(&c.level), "message": c.message, "fix": c.fix}))
        .collect();
    json!({"checks": checks})
}

pub fn activity_json(home: &Path, limit: usize) -> Value {
    json!({"entries": activity::recent(home, limit)})
}

/// Absolute path for a user-given path, relative to the current folder.
pub fn absolute(p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|d| d.join(p))
            .unwrap_or_else(|_| p.to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::recent;

    fn demo_home() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        crate::demo::prepare(d.path(), Utc::now()).unwrap();
        d
    }

    #[test]
    fn add_video_writes_sheet_and_logs_the_agent() {
        let d = demo_home();
        let home = d.path().join("data");
        let src = d.path().join("new clip.mp4");
        std::fs::write(&src, b"\x00\x00\x00\x18ftypmp42 clip").unwrap();
        let agent = Actor::agent("test-agent");
        let fields = SheetFields {
            title: Some("A new clip".into()),
            tags: Some(vec!["a".into(), "b".into()]),
            ..Default::default()
        };
        add_video(&home, &agent, "lumen-lab", &src, &fields, false, None).unwrap();
        let q = home.join("channels/lumen-lab/queue");
        assert!(q.join("new clip.mp4").is_file());
        assert!(std::fs::read_to_string(q.join("new clip.yaml"))
            .unwrap()
            .contains("title: A new clip"));
        assert!(src.is_file(), "copy keeps the original");
        let last = recent(&home, 1).pop().unwrap();
        assert_eq!(
            (last.actor.as_str(), last.agent.as_deref()),
            ("agent", Some("test-agent"))
        );
        // twice is refused, as is a missing title
        assert!(add_video(&home, &agent, "lumen-lab", &src, &fields, false, None).is_err());
        assert!(add_video(
            &home,
            &agent,
            "lumen-lab",
            &src,
            &SheetFields::default(),
            false,
            None
        )
        .unwrap_err()
        .contains("title"));
    }

    #[test]
    fn agents_cannot_upload_without_permission() {
        let d = demo_home();
        let home = d.path().join("data");
        // The settings file lives in the config folder; an empty one means no permission.
        if Settings::load().agents_may_upload {
            return; // a developer machine that allows it: nothing to check here
        }
        let err = run(&home, &Actor::agent("x"), "lumen-lab", true, None).unwrap_err();
        assert!(err.contains("may not upload"));
        assert!(!recent(&home, 1)[0].ok);
    }

    #[test]
    fn queue_reports_problems() {
        let d = demo_home();
        let home = d.path().join("data");
        let v = queue(&home, &Actor::new(activity::CLI), "trail-notes").unwrap();
        assert_eq!(v["ready"], 1);
        let missing = v["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["has_sheet"] == false)
            .unwrap();
        assert!(missing["problem"]
            .as_str()
            .unwrap()
            .contains("missing sheet"));
    }
}
