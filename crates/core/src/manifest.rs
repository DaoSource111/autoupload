//! A video's sheet (`<video>.yaml` or `.json` next to it), merged with the channel defaults into a [`Manifest`].
//!
//! Safe by default: privacy is `private`. `public` also needs `--allow-public` (see publisher).

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::channel::opt_str_or_num;

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Localization {
    pub title: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CaptionSpec {
    pub language: String,
    #[serde(default)]
    pub name: String,
    pub path: String,
}

/// The keys a sheet may contain. Any other key is an error.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sheet {
    pub title: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    #[serde(default, deserialize_with = "opt_str_or_num")]
    pub category_id: Option<String>,
    pub default_language: Option<String>,
    pub default_audio_language: Option<String>,
    pub made_for_kids: Option<bool>,
    pub contains_synthetic_media: Option<bool>,
    pub notify_subscribers: Option<bool>,
    #[serde(default, deserialize_with = "opt_str_or_num")]
    pub playlist_id: Option<String>,
    pub privacy: Option<String>,
    pub publish_at: Option<String>,
    pub thumbnail: Option<String>,
    pub localizations: Option<BTreeMap<String, Localization>>,
    pub captions: Option<Vec<CaptionSpec>>,
}

pub const SHEET_KEYS: &[&str] = &[
    "title",
    "description",
    "tags",
    "category_id",
    "default_language",
    "default_audio_language",
    "made_for_kids",
    "contains_synthetic_media",
    "notify_subscribers",
    "playlist_id",
    "privacy",
    "publish_at",
    "thumbnail",
    "localizations",
    "captions",
];

pub fn read_sheet(path: &Path) -> Result<Sheet, String> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let text = std::fs::read_to_string(path).map_err(|e| format!("{name}: {e}"))?;
    let is_json = path
        .extension()
        .map(|e| e.eq_ignore_ascii_case("json"))
        .unwrap_or(false);
    let value: serde_json::Value = if is_json {
        serde_json::from_str(&text).map_err(|e| format!("{name}: {e}"))?
    } else {
        let y: serde_yaml::Value =
            serde_yaml::from_str(&text).map_err(|e| format!("{name}: {e}"))?;
        serde_json::to_value(y).map_err(|e| format!("{name}: {e}"))?
    };
    let map = value.as_object().ok_or_else(|| {
        format!("{name}: the sheet must be a mapping with at least a 'title' key")
    })?;
    for key in map.keys() {
        if !SHEET_KEYS.contains(&key.as_str()) {
            return Err(format!(
                "{name}: unknown key '{key}' (allowed keys: {})",
                sorted_keys()
            ));
        }
    }
    serde_json::from_value(value).map_err(|e| format!("{name}: {e}"))
}

fn sorted_keys() -> String {
    let mut k = SHEET_KEYS.to_vec();
    k.sort();
    k.join(", ")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Privacy {
    Private,
    Unlisted,
    Public,
}

impl Privacy {
    pub fn parse(s: &str) -> Result<Privacy, String> {
        match s {
            "private" => Ok(Privacy::Private),
            "unlisted" => Ok(Privacy::Unlisted),
            "public" => Ok(Privacy::Public),
            _ => Err(format!(
                "privacy must be private, unlisted or public, not {s:?}"
            )),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Privacy::Private => "private",
            Privacy::Unlisted => "unlisted",
            Privacy::Public => "public",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Caption {
    pub language: String,
    pub name: String,
    pub path: PathBuf,
}

/// Everything needed to upload one video, with absolute paths.
#[derive(Debug, Clone)]
pub struct Manifest {
    pub id: String,
    pub video: PathBuf,
    pub thumbnail: Option<PathBuf>,
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    pub category_id: String,
    pub default_language: String,
    pub default_audio_language: String,
    pub privacy: Privacy,
    pub publish_at: Option<DateTime<Utc>>,
    pub made_for_kids: bool,
    pub contains_synthetic_media: Option<bool>,
    pub notify_subscribers: bool,
    pub localizations: BTreeMap<String, Localization>,
    pub captions: Vec<Caption>,
    pub playlist_id: Option<String>,
}

/// ISO 8601 with an offset (`2026-11-21T08:00:00+00:00`), or without one (taken as UTC).
pub fn parse_publish_at(s: &str) -> Result<DateTime<Utc>, String> {
    let s = s.trim();
    if let Ok(t) = DateTime::parse_from_rfc3339(s) {
        return Ok(t.with_timezone(&Utc));
    }
    if let Ok(t) = DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%:z") {
        return Ok(t.with_timezone(&Utc));
    }
    for f in [
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
    ] {
        if let Ok(n) = NaiveDateTime::parse_from_str(s, f) {
            return Ok(Utc.from_utc_datetime(&n));
        }
    }
    Err(format!(
        "publish_at {s:?} is not an ISO 8601 date, e.g. 2026-11-21T08:00:00+00:00"
    ))
}

fn check_lang(v: &str) -> Result<(), String> {
    let mut parts = v.split('-');
    let first = parts.next().unwrap_or("");
    let ok_first = (2..=3).contains(&first.len()) && first.bytes().all(|b| b.is_ascii_lowercase());
    let ok_rest =
        parts.all(|p| (2..=8).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_alphanumeric()));
    if ok_first && ok_rest {
        Ok(())
    } else {
        Err(format!(
            "invalid language code: {v:?} (use BCP-47, e.g. es, en, pt-BR)"
        ))
    }
}

fn check_title(v: &str) -> Result<(), String> {
    if v.trim().is_empty() {
        return Err("title is empty".into());
    }
    let n = v.chars().count();
    if n > 100 {
        return Err(format!("title has {n} characters (maximum 100)"));
    }
    if v.contains('<') || v.contains('>') {
        return Err("title cannot contain < or >".into());
    }
    Ok(())
}

impl Manifest {
    /// The same rules as 1.x: YouTube's limits on title, description and tags, BCP-47 languages,
    /// publish_at only for private videos and in the future, no duplicate captions.
    pub fn validate(&self, now: DateTime<Utc>) -> Result<(), String> {
        if !valid_id(&self.id) {
            return Err(format!("invalid id {:?}", self.id));
        }
        check_title(&self.title)?;
        if self.description.len() > 5000 {
            return Err("description exceeds 5000 bytes".into());
        }
        if self.description.contains('<') || self.description.contains('>') {
            return Err("description cannot contain < or >".into());
        }
        let total: usize = self
            .tags
            .iter()
            .map(|t| t.chars().count() + if t.contains(' ') { 2 } else { 0 })
            .sum::<usize>()
            + self.tags.len().saturating_sub(1);
        if total > 500 {
            return Err(format!("tags add up to {total} characters (maximum 500)"));
        }
        check_lang(&self.default_language)?;
        check_lang(&self.default_audio_language)?;
        for (lang, loc) in &self.localizations {
            check_lang(lang)?;
            check_title(&loc.title)?;
        }
        if self.localizations.contains_key(&self.default_language) {
            return Err("do not repeat the default language inside localizations".into());
        }
        if let Some(at) = self.publish_at {
            if self.privacy != Privacy::Private {
                return Err(
                    "publish_at requires privacy: private (YouTube publishes it at that time)"
                        .into(),
                );
            }
            if at <= now {
                return Err("publish_at must be in the future".into());
            }
        }
        let mut seen = std::collections::HashSet::new();
        for c in &self.captions {
            check_lang(&c.language)?;
            if c.name.chars().count() > 150 {
                return Err("caption name exceeds 150 characters".into());
            }
            if !seen.insert((c.language.clone(), c.name.clone())) {
                return Err(format!(
                    "duplicate caption ({}, {}): YouTube would respond with captionExists",
                    c.language, c.name
                ));
            }
        }
        Ok(())
    }

    /// (body, part) for videos.insert. Privacy is always explicit: the API uses `public` when it is omitted.
    pub fn insert_body(&self) -> (Value, String) {
        let mut status = json!({"privacyStatus": self.privacy.as_str(), "selfDeclaredMadeForKids": self.made_for_kids});
        if let Some(at) = self.publish_at {
            status["publishAt"] = json!(at.format("%Y-%m-%dT%H:%M:%SZ").to_string());
        }
        if let Some(ai) = self.contains_synthetic_media {
            status["containsSyntheticMedia"] = json!(ai);
        }
        let mut body = json!({
            "snippet": {
                "title": self.title,
                "description": self.description,
                "tags": self.tags,
                "categoryId": self.category_id,
                "defaultLanguage": self.default_language,
                "defaultAudioLanguage": self.default_audio_language,
            },
            "status": status,
        });
        let mut parts = vec!["snippet", "status"];
        if !self.localizations.is_empty() {
            let locs: serde_json::Map<String, Value> = self
                .localizations
                .iter()
                .map(|(k, l)| {
                    (
                        k.clone(),
                        json!({"title": l.title, "description": l.description}),
                    )
                })
                .collect();
            body["localizations"] = Value::Object(locs);
            parts.push("localizations");
        }
        (body, parts.join(","))
    }
}

pub fn valid_id(id: &str) -> bool {
    let b = id.as_bytes();
    (2..=61).contains(&b.len())
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn sample() -> Manifest {
        Manifest {
            id: "ep-1".into(),
            video: PathBuf::from("/q/ep-1.mp4"),
            thumbnail: None,
            title: "Hello".into(),
            description: "Text".into(),
            tags: vec!["one".into()],
            category_id: "27".into(),
            default_language: "en".into(),
            default_audio_language: "en".into(),
            privacy: Privacy::Private,
            publish_at: None,
            made_for_kids: false,
            contains_synthetic_media: None,
            notify_subscribers: false,
            localizations: BTreeMap::new(),
            captions: vec![],
            playlist_id: None,
        }
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 20, 12, 0, 0).unwrap()
    }

    #[test]
    fn valid_sample() {
        sample().validate(now()).unwrap();
    }

    #[test]
    fn rules() {
        let mut m = sample();
        m.title = "x".repeat(101);
        assert!(m.validate(now()).unwrap_err().contains("maximum 100"));
        let mut m = sample();
        m.title = "a <b>".into();
        assert!(m.validate(now()).is_err());
        let mut m = sample();
        m.publish_at = Some(now() - chrono::Duration::hours(1));
        assert!(m.validate(now()).unwrap_err().contains("future"));
        let mut m = sample();
        m.privacy = Privacy::Public;
        m.publish_at = Some(now() + chrono::Duration::hours(1));
        assert!(m.validate(now()).unwrap_err().contains("private"));
        let mut m = sample();
        m.default_language = "english".into();
        assert!(m.validate(now()).unwrap_err().contains("BCP-47"));
        let mut m = sample();
        m.tags = vec!["x".repeat(300), "y".repeat(300)];
        assert!(m.validate(now()).unwrap_err().contains("500"));
    }

    #[test]
    fn insert_body_is_private_with_publish_at_in_utc() {
        let mut m = sample();
        m.publish_at = Some(Utc.with_ymd_and_hms(2026, 11, 21, 8, 0, 0).unwrap());
        m.contains_synthetic_media = Some(true);
        let (body, part) = m.insert_body();
        assert_eq!(part, "snippet,status");
        assert_eq!(body["status"]["privacyStatus"], "private");
        assert_eq!(body["status"]["publishAt"], "2026-11-21T08:00:00Z");
        assert_eq!(body["status"]["containsSyntheticMedia"], true);
    }

    #[test]
    fn publish_at_formats() {
        let want = Utc.with_ymd_and_hms(2026, 11, 21, 8, 0, 0).unwrap();
        assert_eq!(parse_publish_at("2026-11-21T08:00:00+00:00").unwrap(), want);
        assert_eq!(parse_publish_at("2026-11-21T03:00:00-05:00").unwrap(), want);
        assert_eq!(parse_publish_at("2026-11-21T08:00").unwrap(), want);
        assert!(parse_publish_at("tomorrow").is_err());
    }

    #[test]
    fn sheet_unknown_key_is_named() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("a.yaml");
        std::fs::write(&p, "titulo: A\n").unwrap();
        assert!(read_sheet(&p).unwrap_err().contains("titulo"));
        std::fs::write(
            &p,
            "title: A\ncategory_id: 24\npublish_at: 2026-12-01T08:00:00+00:00\n",
        )
        .unwrap();
        let s = read_sheet(&p).unwrap();
        assert_eq!(s.category_id.as_deref(), Some("24"));
        assert_eq!(s.publish_at.as_deref(), Some("2026-12-01T08:00:00+00:00"));
    }
}
