//! Channels: per-channel configuration (`channel.yaml`) and the next free publishing slots.

use chrono::{
    DateTime, Duration, FixedOffset, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, TimeZone,
    Timelike, Utc,
};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub const TEMPLATE: &str = r#"# Configuration for channel '{name}'. See README.md, section "channel.yaml".
profile: {name}                  # OAuth token name; created with: autoupload auth {name} login
client_secret: ../../config/client_secret.json   # Google Cloud desktop OAuth credential (can be the same for every channel)
timezone: "+00:00"               # zone of the hours below: a name such as Europe/Madrid or America/Lima (follows daylight saving), or a fixed offset such as +01:00
hours: ["08:00"]                 # publishing hours for each day (one or several)
days_between: 1                  # 1 = every day · 2 = every other day · 7 = once a week
min_lead_hours: 3                # never schedule less than this many hours from now
max_per_run: 5                   # maximum videos per run (the API limits daily uploads)
language: en                     # language of the title, description and audio
category: "27"                   # 24 Entertainment · 27 Education · 28 Science & Technology · 22 People & Blogs
made_for_kids: false             # "made for kids" declaration (required by law)
contains_ai: null                # true if the video has realistic synthetic voice or imagery; null = do not declare
notify_subscribers: false        # notify subscribers when publishing
playlist: null                   # ID of a playlist (PL...) where each video is added
base_tags: []                    # tags added to every video
description_footer: ""           # text appended to the end of every description
"#;

/// Accepts `"27"` and `27` alike for ID-like fields.
pub(crate) fn str_or_num<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum V {
        S(String),
        I(i64),
    }
    Ok(match V::deserialize(d)? {
        V::S(s) => s,
        V::I(i) => i.to_string(),
    })
}

pub(crate) fn opt_str_or_num<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum V {
        S(String),
        I(i64),
    }
    Ok(Option::<V>::deserialize(d)?.map(|v| match v {
        V::S(s) => s,
        V::I(i) => i.to_string(),
    }))
}

fn d_secret() -> String {
    "../../config/client_secret.json".into()
}
fn d_tz() -> String {
    "+00:00".into()
}
fn d_hours() -> Vec<String> {
    vec!["08:00".into()]
}
fn d_one() -> u32 {
    1
}
fn d_lead() -> f64 {
    3.0
}
fn d_five() -> u32 {
    5
}
fn d_en() -> String {
    "en".into()
}
fn d_cat() -> String {
    "27".into()
}

/// `channel.yaml` as written by the user. Unknown keys are rejected, so typos fail loudly.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelConfig {
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default = "d_secret")]
    pub client_secret: String,
    #[serde(default = "d_tz")]
    pub timezone: String,
    #[serde(default = "d_hours")]
    pub hours: Vec<String>,
    #[serde(default = "d_one")]
    pub days_between: u32,
    #[serde(default = "d_lead")]
    pub min_lead_hours: f64,
    #[serde(default = "d_five")]
    pub max_per_run: u32,
    #[serde(default = "d_en")]
    pub language: String,
    #[serde(default = "d_cat", deserialize_with = "str_or_num")]
    pub category: String,
    #[serde(default)]
    pub made_for_kids: bool,
    #[serde(default)]
    pub contains_ai: Option<bool>,
    #[serde(default)]
    pub notify_subscribers: bool,
    #[serde(default)]
    pub playlist: Option<String>,
    #[serde(default)]
    pub base_tags: Vec<String>,
    #[serde(default)]
    pub description_footer: String,
}

impl Default for ChannelConfig {
    fn default() -> Self {
        serde_yaml::from_str("{}").expect("defaults")
    }
}

/// Time zone of a channel's hours: an IANA name (follows daylight saving) or a fixed offset.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Zone {
    Fixed(FixedOffset),
    Named(chrono_tz::Tz),
}

impl Zone {
    pub fn parse(s: &str) -> Result<Zone, String> {
        let b = s.as_bytes();
        if b.len() == 6 && (b[0] == b'+' || b[0] == b'-') && b[3] == b':' {
            let hh: i32 = s[1..3].parse().map_err(|_| bad_zone(s))?;
            let mm: i32 = s[4..6].parse().map_err(|_| bad_zone(s))?;
            let secs = (hh * 3600 + mm * 60) * if b[0] == b'-' { -1 } else { 1 };
            return FixedOffset::east_opt(secs)
                .map(Zone::Fixed)
                .ok_or_else(|| bad_zone(s));
        }
        s.parse::<chrono_tz::Tz>()
            .map(Zone::Named)
            .map_err(|_| bad_zone(s))
    }

    /// Wall-clock time of an instant in this zone.
    pub fn local(&self, t: DateTime<Utc>) -> NaiveDateTime {
        match self {
            Zone::Fixed(o) => t.with_timezone(o).naive_local(),
            Zone::Named(z) => t.with_timezone(z).naive_local(),
        }
    }

    /// Instant of a wall-clock time. Ambiguous times (autumn) take the first one; a time that does not exist
    /// (inside the spring jump) moves forward by the size of the jump, as clocks do.
    pub fn resolve(&self, local: NaiveDateTime) -> DateTime<Utc> {
        fn pick<T: TimeZone>(tz: &T, local: NaiveDateTime) -> DateTime<Utc> {
            // Gaps are one hour almost everywhere, 30 minutes in a few zones.
            for shift in [0, 60, 30] {
                let probe = local + Duration::minutes(shift);
                match tz.from_local_datetime(&probe) {
                    LocalResult::Single(t) => return t.with_timezone(&Utc),
                    LocalResult::Ambiguous(a, _) => return a.with_timezone(&Utc),
                    LocalResult::None => continue,
                }
            }
            Utc.from_utc_datetime(&local)
        }
        match self {
            Zone::Fixed(o) => pick(o, local),
            Zone::Named(z) => pick(z, local),
        }
    }
}

fn bad_zone(s: &str) -> String {
    format!("timezone {s:?} is neither a zone name like Europe/Madrid nor an offset like +01:00")
}

fn parse_hour(h: &str) -> Result<NaiveTime, String> {
    let ok = h.len() == 5 && h.as_bytes()[2] == b':';
    let t = if ok {
        NaiveTime::parse_from_str(h, "%H:%M").ok()
    } else {
        None
    };
    t.ok_or_else(|| format!("invalid hour: {h:?} (use HH:MM)"))
}

/// A channel as loaded from disk.
#[derive(Debug, Clone)]
pub struct Channel {
    pub name: String,
    pub folder: PathBuf,
    pub cfg: ChannelConfig,
    pub zone: Zone,
    pub hours: Vec<NaiveTime>,
    pub profile: String,
    pub client_secret: PathBuf,
}

impl Channel {
    pub fn from_config(name: &str, folder: &Path, cfg: ChannelConfig) -> Result<Channel, String> {
        let zone = Zone::parse(&cfg.timezone)?;
        if cfg.hours.is_empty() {
            return Err("hours cannot be empty".into());
        }
        let mut hours = cfg
            .hours
            .iter()
            .map(|h| parse_hour(h))
            .collect::<Result<Vec<_>, _>>()?;
        hours.sort();
        hours.dedup();
        if !(1..=30).contains(&cfg.days_between) {
            return Err("days_between must be between 1 and 30".into());
        }
        if cfg.min_lead_hours < 0.0 || !cfg.min_lead_hours.is_finite() {
            return Err("min_lead_hours cannot be negative".into());
        }
        if !(1..=50).contains(&cfg.max_per_run) {
            return Err("max_per_run must be between 1 and 50".into());
        }
        let secret = PathBuf::from(&cfg.client_secret);
        let client_secret = if secret.is_absolute() {
            secret
        } else {
            normalize(&folder.join(secret))
        };
        Ok(Channel {
            name: name.to_string(),
            folder: folder.to_path_buf(),
            profile: cfg.profile.clone().unwrap_or_else(|| name.to_string()),
            zone,
            hours,
            client_secret,
            cfg,
        })
    }

    pub fn queue(&self) -> PathBuf {
        self.folder.join("queue")
    }
    pub fn uploaded(&self) -> PathBuf {
        self.folder.join("uploaded")
    }
    pub fn state_db(&self) -> PathBuf {
        self.folder.join("state.sqlite")
    }
    pub fn channel_id_file(&self) -> PathBuf {
        self.folder.join("channel_id.txt")
    }
    pub fn config_file(&self) -> PathBuf {
        self.folder.join("channel.yaml")
    }

    pub fn expected_channel_id(&self) -> Option<String> {
        std::fs::read_to_string(self.channel_id_file())
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    /// e.g. "08:00, 19:00 every 1 day(s) (Europe/Madrid)"
    pub fn schedule_text(&self) -> String {
        let every = match self.cfg.days_between {
            1 => "every day".to_string(),
            7 => "every week".to_string(),
            n => format!("every {n} days"),
        };
        format!(
            "{} {every} · {}",
            self.cfg.hours.join(", "),
            self.cfg.timezone
        )
    }
}

/// Lexically removes `.` and `..` (the folder may not exist yet, so no canonicalize).
fn normalize(p: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

pub fn valid_name(name: &str) -> bool {
    let b = name.as_bytes();
    !b.is_empty()
        && b.len() <= 41
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_' || *c == b'-')
}

pub fn channels_dir(base: &Path) -> PathBuf {
    base.join("channels")
}

pub fn list_channels(base: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(channels_dir(base))
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().join("channel.yaml").is_file())
                .filter_map(|e| e.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

pub fn load_channel(name: &str, base: &Path) -> Result<Channel, String> {
    let folder = channels_dir(base).join(name);
    let file = folder.join("channel.yaml");
    let text = std::fs::read_to_string(&file).map_err(|_| {
        format!(
            "channel '{name}' does not exist ({}). Create it with: autoupload channel new {name}",
            file.display()
        )
    })?;
    let value: serde_yaml::Value =
        serde_yaml::from_str(&text).map_err(|e| format!("{}: {e}", file.display()))?;
    let cfg = match value {
        serde_yaml::Value::Null => ChannelConfig::default(),
        v @ serde_yaml::Value::Mapping(_) => {
            serde_yaml::from_value(v).map_err(|e| format!("{}: {e}", file.display()))?
        }
        _ => {
            return Err(format!(
                "{}: channel.yaml must contain a mapping of settings",
                file.display()
            ))
        }
    };
    Channel::from_config(name, &folder, cfg).map_err(|e| format!("{}: {e}", file.display()))
}

pub fn create_channel(name: &str, base: &Path) -> Result<PathBuf, String> {
    if !valid_name(name) {
        return Err("channel name may only contain lowercase letters, digits, - and _".into());
    }
    let folder = channels_dir(base).join(name);
    if folder.join("channel.yaml").exists() {
        return Err(format!("channel '{name}' already exists"));
    }
    for sub in ["queue", "uploaded"] {
        std::fs::create_dir_all(folder.join(sub)).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        folder.join("channel.yaml"),
        TEMPLATE.replace("{name}", name),
    )
    .map_err(|e| e.to_string())?;
    Ok(folder)
}

// --- dates -----------------------------------------------------------------------------------------

fn parse_api_time(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

/// Instants already used on the channel: scheduled (publishAt) and published (publishedAt) videos.
pub fn booked_from_videos(videos: &[serde_json::Value]) -> Vec<DateTime<Utc>> {
    let mut out = Vec::new();
    for v in videos {
        let privacy = v["status"]["privacyStatus"].as_str().unwrap_or("");
        let when = match privacy {
            "private" => v["status"]["publishAt"].as_str(),
            "public" | "unlisted" => v["snippet"]["publishedAt"].as_str(),
            _ => None,
        };
        if let Some(t) = when.and_then(parse_api_time) {
            out.push(t);
        }
    }
    out
}

fn minute(t: NaiveDateTime) -> NaiveDateTime {
    t.with_second(0)
        .and_then(|t| t.with_nanosecond(0))
        .unwrap_or(t)
}

/// The next `n` free slots. A slot is free if nobody uses that same time on that day and, with
/// `days_between > 1`, there is no upload less than `days_between` days away (another time on the same day is fine).
pub fn next_slots(
    ch: &Channel,
    n: usize,
    booked: &[DateTime<Utc>],
    now: DateTime<Utc>,
) -> Vec<DateTime<Utc>> {
    let lead = Duration::milliseconds((ch.cfg.min_lead_hours * 3_600_000.0) as i64);
    let earliest = now + lead;
    let mut used: HashSet<NaiveDateTime> =
        booked.iter().map(|b| minute(ch.zone.local(*b))).collect();
    let mut days: HashSet<NaiveDate> = used.iter().map(|u| u.date()).collect();
    let db = ch.cfg.days_between as i64;
    let mut out = Vec::with_capacity(n);
    let mut day = ch.zone.local(earliest).date();
    for _ in 0..(3660 * ch.cfg.days_between.max(1)) {
        if out.len() >= n {
            break;
        }
        for h in &ch.hours {
            if out.len() >= n {
                break;
            }
            let local = NaiveDateTime::new(day, *h);
            let at = ch.zone.resolve(local);
            if at <= earliest || used.contains(&local) {
                continue;
            }
            if db > 1
                && days.iter().any(|d| {
                    let diff = (local.date() - *d).num_days().abs();
                    diff > 0 && diff < db
                })
            {
                continue;
            }
            out.push(at);
            used.insert(local);
            days.insert(day);
        }
        day = match day.succ_opt() {
            Some(d) => d,
            None => break,
        };
    }
    out
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    fn ch(tz: &str, hours: &[&str], days_between: u32) -> Channel {
        let mut cfg = ChannelConfig::default();
        cfg.timezone = tz.into();
        cfg.hours = hours.iter().map(|s| s.to_string()).collect();
        cfg.days_between = days_between;
        Channel::from_config("t", Path::new("/data/channels/t"), cfg).unwrap()
    }

    fn utc(y: i32, m: u32, d: u32, h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, 0, 0).unwrap()
    }

    #[test]
    fn template_parses_to_defaults() {
        let cfg: ChannelConfig = serde_yaml::from_str(&TEMPLATE.replace("{name}", "x")).unwrap();
        assert_eq!(cfg.hours, vec!["08:00"]);
        assert_eq!(cfg.category, "27");
        assert_eq!(cfg.days_between, 1);
    }

    #[test]
    fn unknown_key_is_rejected() {
        let e = serde_yaml::from_str::<ChannelConfig>("hora: 8")
            .unwrap_err()
            .to_string();
        assert!(e.contains("hora"), "{e}");
    }

    #[test]
    fn category_accepts_number() {
        let cfg: ChannelConfig = serde_yaml::from_str("category: 24").unwrap();
        assert_eq!(cfg.category, "24");
    }

    #[test]
    fn slots_skip_booked_and_lead_time() {
        let c = ch("+00:00", &["08:00"], 1);
        let r = next_slots(&c, 3, &[utc(2026, 11, 22, 8)], utc(2026, 11, 20, 12));
        let days: Vec<u32> = r.iter().map(chrono::Datelike::day).collect();
        assert_eq!(days, vec![21, 23, 24]);
    }

    #[test]
    fn slots_every_other_day() {
        let c = ch("+00:00", &["08:00", "19:00"], 2);
        let r = next_slots(&c, 4, &[utc(2026, 11, 21, 8)], utc(2026, 11, 20, 12));
        let got: Vec<(u32, u32)> = r
            .iter()
            .map(|t| (chrono::Datelike::day(t), t.hour()))
            .collect();
        assert_eq!(got, vec![(21, 19), (23, 8), (23, 19), (25, 8)]);
    }

    #[test]
    fn named_zone_keeps_local_hour_across_daylight_saving() {
        // Europe moves the clocks back at 03:00 on 2026-10-25.
        let c = ch("Europe/Madrid", &["08:00"], 1);
        let r = next_slots(&c, 3, &[], utc(2026, 10, 23, 12));
        let hours: Vec<u32> = r.iter().map(|t| t.hour()).collect();
        assert_eq!(hours, vec![6, 7, 7]);
        assert!(r.iter().all(|t| c.zone.local(*t).hour() == 8));
    }

    #[test]
    fn nonexistent_spring_time_moves_forward() {
        // 2027-03-28 02:30 does not exist in Madrid (02:00 → 03:00): it becomes 03:30 local = 01:30 UTC.
        let z = Zone::parse("Europe/Madrid").unwrap();
        let local = NaiveDate::from_ymd_opt(2027, 3, 28)
            .unwrap()
            .and_hms_opt(2, 30, 0)
            .unwrap();
        assert_eq!(
            z.resolve(local),
            utc(2027, 3, 28, 1) + Duration::minutes(30)
        );
    }

    #[test]
    fn bad_zone_and_hours_are_rejected() {
        let mut cfg = ChannelConfig::default();
        cfg.timezone = "Mars/Base".into();
        assert!(Channel::from_config("t", Path::new("/x"), cfg)
            .unwrap_err()
            .contains("neither a zone name"));
        let mut cfg = ChannelConfig::default();
        cfg.hours = vec!["8:00".into()];
        assert!(Channel::from_config("t", Path::new("/x"), cfg)
            .unwrap_err()
            .contains("HH:MM"));
    }

    #[test]
    fn booked_dates_from_api() {
        let v = serde_json::json!([
            {"status": {"privacyStatus": "private", "publishAt": "2026-12-02T13:00:00Z"}, "snippet": {}},
            {"status": {"privacyStatus": "public"}, "snippet": {"publishedAt": "2026-11-01T10:00:00Z"}},
            {"status": {"privacyStatus": "private"}, "snippet": {}}
        ]);
        assert_eq!(booked_from_videos(v.as_array().unwrap()).len(), 2);
    }

    #[test]
    fn create_list_and_load() {
        let d = tempfile::tempdir().unwrap();
        create_channel("main", d.path()).unwrap();
        assert!(create_channel("Bad Name", d.path()).is_err());
        assert_eq!(list_channels(d.path()), vec!["main"]);
        let c = load_channel("main", d.path()).unwrap();
        assert_eq!(c.profile, "main");
        assert!(c
            .client_secret
            .ends_with(Path::new("config").join("client_secret.json")));
        assert!(c.client_secret.starts_with(d.path()));
    }
}
