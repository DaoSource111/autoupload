//! Demo mode: sample channels and an offline YouTube, so the app and the CLI can be tried (and screenshotted)
//! without a Google account. Nothing leaves the computer and the user's real data folder is never touched.
//!
//! Turned on with `--demo` (or `AUTOUPLOAD_DEMO=1`): the data and config folders point to a fresh folder in the
//! system temp directory, and [`crate::client::api_for`] returns a [`DemoApi`] instead of the real client.

use chrono::{DateTime, Duration, Timelike, Utc};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::activity::{self, Entry};
use crate::channel::Channel;
use crate::youtube::{Api, ApiError, ChannelInfo};

pub const ENV: &str = "AUTOUPLOAD_DEMO";

pub fn active() -> bool {
    std::env::var(ENV).map(|v| v == "1").unwrap_or(false)
}

/// The demo folder in the system temp directory. Delete it to start the demo over.
pub fn folder() -> PathBuf {
    std::env::temp_dir().join("AutoUpload-demo")
}

/// Points this process at the demo folder, creating it the first time. Call before any other thread starts.
/// The folder is kept between runs, so a sequence of demo commands (or an agent's calls) builds on each other.
pub fn enable() -> std::io::Result<PathBuf> {
    let root = folder();
    if !root.join("data").join("channels").is_dir() {
        let _ = std::fs::remove_dir_all(&root);
        prepare(&root, Utc::now())?;
    }
    std::env::set_var(ENV, "1");
    std::env::set_var("AUTOUPLOAD_HOME", root.join("data"));
    std::env::set_var("AUTOUPLOAD_CONFIG_DIR", root.join("config"));
    Ok(root)
}

struct DemoChannel {
    name: &'static str,
    id: &'static str,
    title: &'static str,
    yaml: &'static str,
    /// (file stem, sheet or None)
    queue: &'static [(&'static str, Option<&'static str>)],
}

const CHANNELS: &[DemoChannel] = &[
    DemoChannel {
        name: "lumen-lab",
        id: "UCdemo0LumenLab00000000",
        title: "Lumen Lab (demo)",
        yaml: "profile: lumen-lab\nclient_secret: ../../config/client_secret.json\ntimezone: Europe/Madrid\nhours: [\"18:00\"]\ndays_between: 1\nmin_lead_hours: 3\nmax_per_run: 5\nlanguage: en\ncategory: \"28\"\nmade_for_kids: false\ncontains_ai: false\nnotify_subscribers: true\nplaylist: null\nbase_tags: [science, lumen lab]\ndescription_footer: \"New experiments every evening at 18:00.\"\n",
        queue: &[
            ("why-the-sky-is-blue", Some("title: Why is the sky blue? A 5-minute experiment\ndescription: |\n  Rayleigh scattering with a glass of water and a torch.\ntags: [physics, light, experiment]\n")),
            ("salt-crystals-timelapse", Some("title: Growing salt crystals — 10 days in 60 seconds\ndescription: |\n  A time-lapse of a saturated salt solution.\ntags: [chemistry, timelapse]\n")),
            ("homemade-spectroscope", Some("title: Build a spectroscope from an old CD\ndescription: |\n  Split the light of any lamp into its colours.\ntags: [optics, diy]\n")),
            ("magnet-levitation", Some("title: Can a pencil levitate? Diamagnetism at home\ndescription: |\n  Graphite, four magnets and a lot of patience.\ntags: [magnetism, physics]\n")),
        ],
    },
    DemoChannel {
        name: "trail-notes",
        id: "UCdemo0TrailNotes000000",
        title: "Trail Notes (demo)",
        yaml: "profile: trail-notes\nclient_secret: ../../config/client_secret.json\ntimezone: America/New_York\nhours: [\"09:00\", \"17:00\"]\ndays_between: 2\nmin_lead_hours: 3\nmax_per_run: 3\nlanguage: en\ncategory: \"19\"\nmade_for_kids: false\ncontains_ai: null\nnotify_subscribers: false\nplaylist: null\nbase_tags: [hiking]\ndescription_footer: \"\"\n",
        queue: &[
            ("blue-ridge-sunrise", Some("title: Sunrise on the Blue Ridge, unedited\ntags: [appalachia, sunrise]\n")),
            ("gear-check-autumn", None),
        ],
    },
];

/// A few uploads already on each demo channel: (days from now, hour, privacy, title).
fn existing(channel: &str) -> &'static [(i64, u32, &'static str, &'static str)] {
    match channel {
        "UCdemo0LumenLab00000000" => &[
            (-3, 17, "public", "Soap film colours, explained"),
            (-2, 17, "public", "The fastest way to cool a drink (tested)"),
            (-1, 17, "public", "Pepper, water and a drop of soap"),
            (0, 17, "private", "How does a rainbow form?"),
            (1, 17, "private", "Make a cloud in a bottle"),
        ],
        _ => &[
            (-4, 14, "public", "Three days on the Long Trail"),
            (1, 21, "private", "Ultralight kitchen: what I carry"),
        ],
    }
}

fn video_json(id: &str, title: &str, privacy: &str, when: DateTime<Utc>) -> Value {
    let stamp = when.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let mut v = json!({
        "id": id,
        "snippet": {"title": title, "publishedAt": stamp},
        "status": {"privacyStatus": privacy, "uploadStatus": "processed"},
    });
    if privacy == "private" {
        v["status"]["publishAt"] = json!(stamp);
    }
    v
}

/// Uploads the demo channels hold this session, shared by every [`DemoApi`] (keyed by channel ID).
static VIDEOS: Mutex<Option<HashMap<String, Vec<Value>>>> = Mutex::new(None);

fn with_videos<T>(channel_id: &str, f: impl FnOnce(&mut Vec<Value>) -> T) -> T {
    let mut guard = VIDEOS.lock().unwrap_or_else(|p| p.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    let list = map.entry(channel_id.to_string()).or_insert_with(|| {
        let today = Utc::now()
            .with_minute(0)
            .and_then(|t| t.with_second(0))
            .and_then(|t| t.with_nanosecond(0))
            .unwrap_or_else(Utc::now);
        existing(channel_id)
            .iter()
            .enumerate()
            .map(|(i, (days, hour, privacy, title))| {
                let when = (today + Duration::days(*days))
                    .with_hour(*hour)
                    .unwrap_or(today);
                video_json(&format!("demo{i:02}"), title, privacy, when)
            })
            .collect()
    });
    f(list)
}

/// An offline YouTube for the demo channels. Uploads take a couple of seconds and are kept in memory.
pub struct DemoApi {
    channel_id: String,
    title: String,
}

impl DemoApi {
    pub fn for_channel(ch: &Channel) -> DemoApi {
        let id = ch
            .expected_channel_id()
            .unwrap_or_else(|| "UCdemo0Unknown000000000".into());
        let title = CHANNELS
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.title.to_string())
            .unwrap_or_else(|| format!("{} (demo)", ch.name));
        DemoApi {
            channel_id: id,
            title,
        }
    }
}

impl Api for DemoApi {
    fn my_channel(&self) -> Result<ChannelInfo, ApiError> {
        Ok(ChannelInfo {
            id: self.channel_id.clone(),
            title: self.title.clone(),
        })
    }

    fn uploads(&self, max: usize, _parts: &str) -> Result<Vec<Value>, ApiError> {
        Ok(with_videos(&self.channel_id, |v| {
            v.iter().rev().take(max).cloned().collect()
        }))
    }

    fn get_video(&self, id: &str) -> Result<Option<Value>, ApiError> {
        Ok(with_videos(&self.channel_id, |v| {
            v.iter().find(|x| x["id"] == id).cloned()
        }))
    }

    fn insert_video(
        &self,
        body: &Value,
        _part: &str,
        path: &Path,
        _notify: bool,
        _session: Option<&str>,
        on_session: &mut dyn FnMut(&str),
        on_progress: &mut dyn FnMut(f32),
    ) -> Result<String, ApiError> {
        on_session("demo://upload-session");
        for i in 1..=20 {
            std::thread::sleep(std::time::Duration::from_millis(90));
            on_progress(i as f32 / 20.0);
        }
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(with_videos(&self.channel_id, |list| {
            let id = format!("demo{:02}", list.len());
            let mut v = body.clone();
            v["id"] = json!(id);
            v["status"]["uploadStatus"] = json!("uploaded");
            v["fileDetails"] = json!({"fileName": name, "fileSize": size.to_string()});
            list.push(v);
            id
        }))
    }

    fn set_thumbnail(&self, _video_id: &str, _path: &Path) -> Result<(), ApiError> {
        Ok(())
    }

    fn insert_caption(&self, _: &str, _: &str, _: &str, _: &Path) -> Result<(), ApiError> {
        Ok(())
    }

    fn add_to_playlist(&self, _: &str, _: &str) -> Result<(), ApiError> {
        Ok(())
    }
}

/// Writes the demo data folder (`root/data`) and config folder (`root/config`).
pub fn prepare(root: &Path, now: DateTime<Utc>) -> std::io::Result<()> {
    let data = root.join("data");
    let config = root.join("config");
    std::fs::create_dir_all(data.join("config"))?;
    std::fs::create_dir_all(&config)?;
    // Placeholders only: the demo never contacts Google, so these values are never used.
    std::fs::write(
        data.join("config").join("client_secret.json"),
        r#"{"installed":{"client_id":"demo-only.apps.googleusercontent.com","client_secret":"demo-only","auth_uri":"https://accounts.google.com/o/oauth2/auth","token_uri":"https://oauth2.googleapis.com/token","redirect_uris":["http://localhost"]}}"#,
    )?;
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
    for c in CHANNELS {
        let folder = data.join("channels").join(c.name);
        std::fs::create_dir_all(folder.join("queue"))?;
        std::fs::create_dir_all(folder.join("uploaded"))?;
        std::fs::write(folder.join("channel.yaml"), c.yaml)?;
        std::fs::write(folder.join("channel_id.txt"), c.id)?;
        std::fs::write(
            config.join(format!("token-{}.json", c.name)),
            r#"{"token":"demo-only","refresh_token":"demo-only","token_uri":"https://oauth2.googleapis.com/token","client_id":"demo-only.apps.googleusercontent.com","client_secret":"demo-only","scopes":[]}"#,
        )?;
        for (stem, sheet) in c.queue {
            let video = folder.join("queue").join(format!("{stem}.mp4"));
            std::fs::write(
                &video,
                [
                    b"\x00\x00\x00\x18ftypmp42".as_slice(),
                    stem.as_bytes(),
                    &[0u8; 2048],
                ]
                .concat(),
            )?;
            // An hour old, so the queue does not treat it as a file still being copied.
            std::fs::File::options()
                .write(true)
                .open(&video)?
                .set_modified(old)?;
            if let Some(s) = sheet {
                std::fs::write(folder.join("queue").join(format!("{stem}.yaml")), s)?;
            }
        }
    }
    // What an AI agent and the daily task did earlier, as the Activity tab would show it.
    let past = |mins: i64| now - Duration::minutes(mins);
    let entries = [
        (
            past(1440),
            activity::TASK,
            None,
            Some("lumen-lab"),
            "run",
            "Daily run: 1 video scheduled for tomorrow 18:00",
            true,
        ),
        (
            past(52),
            activity::AGENT,
            Some("claude-code"),
            Some("lumen-lab"),
            "add_video",
            "Queued why-the-sky-is-blue.mp4 with title, description and 3 tags",
            true,
        ),
        (
            past(51),
            activity::AGENT,
            Some("claude-code"),
            Some("lumen-lab"),
            "add_video",
            "Queued salt-crystals-timelapse.mp4 with title, description and 2 tags",
            true,
        ),
        (
            past(50),
            activity::AGENT,
            Some("claude-code"),
            Some("lumen-lab"),
            "add_video",
            "Queued homemade-spectroscope.mp4 with title, description and 2 tags",
            true,
        ),
        (
            past(49),
            activity::AGENT,
            Some("claude-code"),
            Some("lumen-lab"),
            "add_video",
            "Queued magnet-levitation.mp4 with title, description and 2 tags",
            true,
        ),
        (
            past(47),
            activity::AGENT,
            Some("claude-code"),
            Some("lumen-lab"),
            "plan",
            "Previewed 4 dates: next free slots from the day after tomorrow at 18:00",
            true,
        ),
        (
            past(46),
            activity::AGENT,
            Some("claude-code"),
            Some("lumen-lab"),
            "upload",
            "Upload refused: AI agents may not upload until a person allows it in the app",
            false,
        ),
        (
            past(12),
            activity::AGENT,
            Some("codex"),
            Some("trail-notes"),
            "queue",
            "Read the queue: 1 ready, 1 without a details file (gear-check-autumn.mp4)",
            true,
        ),
    ];
    for (time, actor, agent, channel, action, summary, ok) in entries {
        activity::append(
            &data,
            &Entry {
                time,
                actor: actor.into(),
                agent: agent.map(String::from),
                channel: channel.map(String::from),
                action: action.into(),
                summary: summary.into(),
                ok,
            },
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::{list_channels, load_channel};
    use crate::queue::queue_items;
    use crate::runner::{calendar_rows, process_queue, RunOptions};

    #[test]
    fn demo_data_loads_and_plans_offline() {
        let d = tempfile::tempdir().unwrap();
        prepare(d.path(), Utc::now()).unwrap();
        let home = d.path().join("data");
        assert_eq!(list_channels(&home), vec!["lumen-lab", "trail-notes"]);
        let ch = load_channel("lumen-lab", &home).unwrap();
        assert_eq!(queue_items(&ch).len(), 4);
        let api = DemoApi::for_channel(&ch);
        assert_eq!(api.my_channel().unwrap().id, "UCdemo0LumenLab00000000");
        assert!(!calendar_rows(&api, 30, Utc::now()).unwrap().is_empty());
        let r = process_queue(&ch, &api, &RunOptions::default(), &mut |_| {}).unwrap();
        assert_eq!(r.plan.len(), 4, "{:?}", r.errors);
        assert_eq!(activity::recent(&home, 100).len(), 8);
    }
}
