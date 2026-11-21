//! Processing a channel's queue. Progress is reported as [`Event`]s, which the CLI prints and the app shows.

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::channel::{booked_from_videos, next_slots, Channel};
use crate::publisher::{publish, write_report, PublishError};
use crate::queue::{build_manifest, is_settling, queue_items, ChannelLock, QueueItem, SETTLE};
use crate::state::State;
use crate::youtube::{Api, ApiError, ErrorKind};

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Info(String),
    /// A queued video and the date it gets (or got).
    Planned {
        file: String,
        title: String,
        when: String,
    },
    Uploading {
        file: String,
    },
    Progress {
        file: String,
        fraction: f32,
    },
    Done {
        file: String,
        url: String,
        when: String,
        verified: bool,
    },
    Warning(String),
    Error(String),
}

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub execute: bool,
    pub limit: Option<usize>,
    pub allow_public: bool,
    pub now: Option<DateTime<Utc>>,
    pub settle: Duration,
}

impl Default for RunOptions {
    fn default() -> Self {
        RunOptions {
            execute: false,
            limit: None,
            allow_public: false,
            now: None,
            settle: SETTLE,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DoneItem {
    pub video: String,
    pub url: String,
    pub when: String,
    pub verified: bool,
}

#[derive(Debug, Default, Clone)]
pub struct RunResult {
    pub done: Vec<DoneItem>,
    pub errors: Vec<String>,
    pub plan: Vec<String>,
}

/// An error that stops the whole run (sign-in, daily quota, wrong channel, another run in progress).
#[derive(Debug, Clone)]
pub struct StopRun {
    pub message: String,
    pub auth: bool,
}

impl std::fmt::Display for StopRun {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

fn stop(msg: String) -> StopRun {
    StopRun {
        message: msg,
        auth: false,
    }
}

fn api_stop(ch: &Channel, e: ApiError) -> StopRun {
    if e.kind == ErrorKind::Auth {
        StopRun {
            message: format!("[{}] Authentication: {}", ch.name, e.message),
            auth: true,
        }
    } else {
        stop(format!("[{}] {e}", ch.name))
    }
}

fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if std::fs::rename(from, to).is_err() {
        std::fs::copy(from, to)?;
        std::fs::remove_file(from)?;
    }
    Ok(())
}

/// Schedules the queue. With `execute = false` it only shows the plan (reads the channel calendar, uploads nothing).
pub fn process_queue(
    ch: &Channel,
    api: &dyn Api,
    opts: &RunOptions,
    ev: &mut dyn FnMut(Event),
) -> Result<RunResult, StopRun> {
    let mut result = RunResult::default();
    let mut ready: Vec<QueueItem> = Vec::new();
    for item in queue_items(ch) {
        if is_settling(&item.video, opts.settle) {
            ev(Event::Info(format!(
                "[{}] {} changed less than {}s ago (still copying?); it waits for the next run",
                ch.name,
                item.file_name(),
                opts.settle.as_secs()
            )));
        } else {
            ready.push(item);
        }
    }
    ready.truncate(opts.limit.unwrap_or(ch.cfg.max_per_run as usize));
    if ready.is_empty() {
        ev(Event::Info(format!("[{}] queue empty", ch.name)));
        return Ok(result);
    }
    let _lock = if opts.execute {
        Some(ChannelLock::acquire(ch).map_err(stop)?)
    } else {
        None
    };

    let yt = api.my_channel().map_err(|e| api_stop(ch, e))?;
    if let Some(expected) = ch.expected_channel_id() {
        if yt.id != expected {
            return Err(StopRun {
                message: format!(
                    "[{}] the token belongs to channel '{}' ({}), not to {expected}. Run: autoupload auth {} login",
                    ch.name, yt.title, yt.id, ch.name
                ),
                auth: true,
            });
        }
    }
    ev(Event::Info(format!(
        "[{}] connected channel: {} ({})",
        ch.name, yt.title, yt.id
    )));

    let now = opts.now.unwrap_or_else(Utc::now);
    let booked = booked_from_videos(
        &api.uploads(200, "snippet,status")
            .map_err(|e| api_stop(ch, e))?,
    );
    let slots = next_slots(ch, ready.len(), &booked, now);
    let state = if opts.execute {
        Some(
            State::open(&ch.state_db())
                .map_err(|e| stop(format!("[{}] state database: {e}", ch.name)))?,
        )
    } else {
        None
    };

    for (item, when) in ready
        .iter()
        .zip(slots.into_iter().map(Some).chain(std::iter::repeat(None)))
    {
        let file = item.file_name();
        let m = match build_manifest(ch, item, when) {
            Ok(m) => m,
            Err(e) => {
                result.errors.push(format!("{file}: {e}"));
                ev(Event::Error(format!("{file}: {e}")));
                continue;
            }
        };
        let when_text = match m.publish_at {
            Some(t) => ch.zone.local(t).format("%Y-%m-%d %H:%M").to_string(),
            None => m.privacy.as_str().to_string(),
        };
        result
            .plan
            .push(format!("{file} → '{}' · {when_text}", m.title));
        ev(Event::Planned {
            file: file.clone(),
            title: m.title.clone(),
            when: when_text.clone(),
        });
        let Some(state) = state.as_ref() else {
            continue;
        };

        ev(Event::Uploading { file: file.clone() });
        let r = {
            let mut cb = |f: f32| {
                ev(Event::Progress {
                    file: file.clone(),
                    fraction: f,
                })
            };
            publish(&m, api, state, true, opts.allow_public, &mut cb)
        };
        let r = match r {
            Ok(r) => r,
            Err(PublishError::Api(e)) if e.is_stop() => {
                return Err(stop(format!(
                    "[{}] YouTube does not accept more uploads today: {e}",
                    ch.name
                )));
            }
            Err(PublishError::Api(e)) if e.kind == ErrorKind::Auth => return Err(api_stop(ch, e)),
            Err(PublishError::Safety(e)) => {
                result.errors.push(format!("{file}: {e}"));
                ev(Event::Error(format!("{file}: blocked: {e}")));
                continue;
            }
            Err(e) => {
                result.errors.push(format!("{file}: {e}"));
                ev(Event::Error(format!(
                    "{file}: {e} (the saved state allows retrying without duplicates)"
                )));
                continue;
            }
        };
        let same_file_other_item = r
            .previous_manifest_id
            .as_deref()
            .map(|p| p != m.id)
            .unwrap_or(false);
        if same_file_other_item
            && r.steps
                .iter()
                .any(|s| s.starts_with("videos.insert (skipped: already uploaded"))
        {
            let msg = format!("{file}: this same file was already uploaded to this channel ({}); it will not be uploaded again. Remove it from the queue.", r.url);
            result.errors.push(msg.clone());
            ev(Event::Error(msg));
            continue;
        }
        for w in &r.warnings {
            ev(Event::Warning(format!("{file}: {w}")));
        }
        let today = ch.zone.local(Utc::now()).format("%Y-%m-%d");
        let dest: PathBuf = ch.uploaded().join(format!("{today}_{}", m.id));
        if let Err(e) =
            std::fs::create_dir_all(&dest).and_then(|_| write_report(&r, &dest.join("report.json")))
        {
            ev(Event::Warning(format!(
                "{file}: uploaded, but the report could not be written: {e}"
            )));
        }
        for p in [
            Some(&item.video),
            item.sheet.as_ref(),
            item.thumbnail.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            if p.exists() {
                if let Some(name) = p.file_name() {
                    if let Err(e) = move_file(p, &dest.join(name)) {
                        ev(Event::Warning(format!(
                            "{}: could not move to {}: {e}",
                            p.display(),
                            dest.display()
                        )));
                    }
                }
            }
        }
        result.done.push(DoneItem {
            video: file.clone(),
            url: r.url.clone(),
            when: when_text.clone(),
            verified: r.verification.ok,
        });
        ev(Event::Done {
            file,
            url: r.url,
            when: when_text,
            verified: r.verification.ok,
        });
    }
    Ok(result)
}

/// Channel uploads between 3 days ago and `days` days from now: (instant, privacy, title).
pub fn calendar_rows(
    api: &dyn Api,
    days: i64,
    now: DateTime<Utc>,
) -> Result<Vec<(DateTime<Utc>, String, String)>, ApiError> {
    let mut rows = Vec::new();
    for v in api.uploads(200, "snippet,status")? {
        for when in booked_from_videos(std::slice::from_ref(&v)) {
            if when >= now - ChronoDuration::days(3) && when <= now + ChronoDuration::days(days) {
                rows.push((
                    when,
                    v["status"]["privacyStatus"]
                        .as_str()
                        .unwrap_or("?")
                        .to_string(),
                    v["snippet"]["title"].as_str().unwrap_or("").to_string(),
                ));
            }
        }
    }
    rows.sort_by_key(|r| r.0);
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::publisher::fake::FakeApi;
    use crate::queue::tests::{channel, put};
    use serde_json::json;

    fn opts(execute: bool) -> RunOptions {
        RunOptions {
            execute,
            settle: Duration::ZERO,
            now: Some(Utc::now()),
            ..Default::default()
        }
    }

    fn scheduled(t: DateTime<Utc>) -> serde_json::Value {
        json!({"id": "X", "snippet": {"title": "already there"},
               "status": {"privacyStatus": "private", "publishAt": t.format("%Y-%m-%dT%H:%M:%SZ").to_string()}})
    }

    fn quiet() -> impl FnMut(Event) {
        |_e: Event| {}
    }

    #[test]
    fn plan_uploads_nothing() {
        let d = tempfile::tempdir().unwrap();
        let c = channel(d.path());
        put(&c, "a", Some("title: A\n"), false);
        let api = FakeApi::new();
        let r = process_queue(&c, &api, &opts(false), &mut quiet()).unwrap();
        assert_eq!(r.plan.len(), 1);
        assert_eq!(api.count("videos.insert"), 0);
        assert!(c.queue().join("a.mp4").exists());
    }

    #[test]
    fn schedules_and_moves_to_uploaded() {
        let d = tempfile::tempdir().unwrap();
        let c = channel(d.path());
        put(&c, "a", Some("title: A\n"), false);
        put(&c, "b", Some("title: B\n"), false);
        put(&c, "c", None, false);
        let api = FakeApi::new();
        let first = next_slots(&c, 1, &[], Utc::now())[0];
        api.existing.borrow_mut().push(scheduled(first));
        let r = process_queue(&c, &api, &opts(true), &mut quiet()).unwrap();
        assert_eq!(r.done.len(), 2);
        assert_eq!(api.count("videos.insert"), 2);
        assert_eq!(r.errors.len(), 1);
        assert!(c.queue().join("c.mp4").exists() && !c.queue().join("a.mp4").exists());
        let reports = std::fs::read_dir(c.uploaded())
            .unwrap()
            .filter(|e| e.as_ref().unwrap().path().join("report.json").exists())
            .count();
        assert_eq!(reports, 2);
        let first_local = c.zone.local(first).format("%Y-%m-%d %H:%M").to_string();
        assert!(
            r.done.iter().all(|d| d.when != first_local),
            "the booked slot must be skipped"
        );
    }

    #[test]
    fn wrong_channel_stops() {
        let d = tempfile::tempdir().unwrap();
        let c = channel(d.path());
        std::fs::write(c.channel_id_file(), "UC_OTHER").unwrap();
        put(&c, "a", Some("title: A\n"), false);
        let e = process_queue(&c, &FakeApi::new(), &opts(true), &mut quiet()).unwrap_err();
        assert!(e.auth && e.message.contains("UC_OTHER"));
        assert!(c.queue().join("a.mp4").exists());
    }

    #[test]
    fn public_requires_permission() {
        let d = tempfile::tempdir().unwrap();
        let c = channel(d.path());
        put(&c, "a", Some("title: A\nprivacy: public\n"), false);
        let r = process_queue(&c, &FakeApi::new(), &opts(true), &mut quiet()).unwrap();
        assert!(!r.errors.is_empty() && r.done.is_empty());
    }

    #[test]
    fn daily_limit_stops_the_run() {
        let d = tempfile::tempdir().unwrap();
        let c = channel(d.path());
        put(&c, "a", Some("title: A\n"), false);
        let api = FakeApi::new().failing(
            "videos.insert",
            vec![ApiError::http(400, Some("uploadLimitExceeded"), "limit")],
        );
        assert!(process_queue(&c, &api, &opts(true), &mut quiet())
            .unwrap_err()
            .message
            .contains("does not accept more uploads"));
    }

    #[test]
    fn same_file_twice_is_not_duplicated() {
        let d = tempfile::tempdir().unwrap();
        let c = channel(d.path());
        put(&c, "a", Some("title: A\n"), false);
        std::fs::copy(c.queue().join("a.mp4"), c.queue().join("a2.mp4")).unwrap();
        std::fs::write(c.queue().join("a2.yaml"), "title: Other\n").unwrap();
        let api = FakeApi::new();
        let r = process_queue(&c, &api, &opts(true), &mut quiet()).unwrap();
        assert_eq!(api.count("videos.insert"), 1);
        assert_eq!(r.done.len(), 1);
        assert_eq!(r.errors.len(), 1);
        assert!(c.queue().join("a2.mp4").exists());
    }

    #[test]
    fn interrupted_item_is_completed_on_next_run() {
        let d = tempfile::tempdir().unwrap();
        let c = channel(d.path());
        put(&c, "a", Some("title: A\nplaylist_id: PL_X\n"), false);
        let api = FakeApi::new().failing(
            "playlistItems.insert",
            vec![ApiError::http(500, None, "boom")],
        );
        let r1 = process_queue(&c, &api, &opts(true), &mut quiet()).unwrap();
        assert!(!r1.errors.is_empty() && c.queue().join("a.mp4").exists());
        let r2 = process_queue(&c, &api, &opts(true), &mut quiet()).unwrap();
        assert_eq!(r2.done.len(), 1);
        assert!(r2.errors.is_empty());
        assert_eq!(api.count("videos.insert"), 1);
        assert!(!c.queue().join("a.mp4").exists());
    }

    #[test]
    fn still_copying_waits_and_lock_blocks() {
        let d = tempfile::tempdir().unwrap();
        let c = channel(d.path());
        put(&c, "a", Some("title: A\n"), false);
        let mut o = opts(true);
        o.settle = Duration::from_secs(120);
        let mut msgs = Vec::new();
        let r = process_queue(&c, &FakeApi::new(), &o, &mut |e| msgs.push(e)).unwrap();
        assert!(
            r.done.is_empty()
                && msgs
                    .iter()
                    .any(|m| matches!(m, Event::Info(s) if s.contains("still copying")))
        );
        let _l = ChannelLock::acquire(&c).unwrap();
        assert!(
            process_queue(&c, &FakeApi::new(), &opts(true), &mut quiet())
                .unwrap_err()
                .message
                .contains("another AutoUpload run")
        );
    }
}
