//! Channel queue: each video in the queue folder with its sheet is scheduled in the next free slot.

use chrono::{DateTime, Utc};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use unicode_normalization::UnicodeNormalization;

use crate::channel::Channel;
use crate::manifest::{parse_publish_at, read_sheet, Caption, Manifest, Privacy};
use crate::validate::{ext_lower, THUMB_EXT, VIDEO_EXT};

/// A video modified this recently may still be copying: it waits for the next run.
pub const SETTLE: Duration = Duration::from_secs(120);
/// A lock older than this is left over from a crashed run.
pub const LOCK_STALE: Duration = Duration::from_secs(6 * 3600);

#[derive(Debug, Clone, PartialEq)]
pub struct QueueItem {
    pub video: PathBuf,
    pub sheet: Option<PathBuf>,
    pub thumbnail: Option<PathBuf>,
}

impl QueueItem {
    /// ASCII slug of the file name, used as the manifest id and in report folder names.
    pub fn id(&self) -> String {
        let stem = self
            .video
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let ascii: String = stem
            .nfkd()
            .filter(|c| c.is_ascii())
            .collect::<String>()
            .to_lowercase();
        let mut slug = String::new();
        let mut dash = false;
        for c in ascii.chars() {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' {
                slug.push(c);
                dash = false;
            } else if !dash {
                slug.push('-');
                dash = true;
            }
        }
        let slug: String = slug.trim_matches('-').chars().take(60).collect();
        let slug = slug.trim_end_matches('-').to_string();
        if slug.len() >= 2 {
            slug
        } else {
            format!(
                "video-{}",
                if slug.is_empty() {
                    "x".to_string()
                } else {
                    slug
                }
            )
        }
    }

    pub fn file_name(&self) -> String {
        self.video
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

fn sibling(video: &Path, exts: &[&str]) -> Option<PathBuf> {
    exts.iter()
        .map(|e| video.with_extension(e))
        .find(|p| p.is_file())
}

pub fn queue_items(ch: &Channel) -> Vec<QueueItem> {
    let Ok(rd) = std::fs::read_dir(ch.queue()) else {
        return vec![];
    };
    let mut videos: Vec<PathBuf> = rd
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && VIDEO_EXT.contains(&ext_lower(p).as_str()))
        .collect();
    videos.sort();
    videos
        .into_iter()
        .map(|v| QueueItem {
            sheet: sibling(&v, &["yaml", "yml", "json"]),
            thumbnail: sibling(&v, THUMB_EXT),
            video: v,
        })
        .collect()
}

/// True if the file changed less than `settle` ago (it may still be being copied into the queue).
pub fn is_settling(path: &Path, settle: Duration) -> bool {
    match std::fs::metadata(path).and_then(|m| m.modified()) {
        Ok(t) => SystemTime::now()
            .duration_since(t)
            .map(|age| age < settle)
            .unwrap_or(true),
        Err(_) => true,
    }
}

/// Only one run at a time per channel. Released when dropped.
#[derive(Debug)]
pub struct ChannelLock {
    path: PathBuf,
}

impl ChannelLock {
    pub fn acquire(ch: &Channel) -> Result<ChannelLock, String> {
        let path = ch.folder.join("run.lock");
        for _ in 0..2 {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut f) => {
                    let _ = writeln!(
                        f,
                        "pid={} started={}",
                        std::process::id(),
                        Utc::now().to_rfc3339()
                    );
                    return Ok(ChannelLock { path });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    let age = std::fs::metadata(&path)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| SystemTime::now().duration_since(t).ok())
                        .unwrap_or_default();
                    if age > LOCK_STALE {
                        let _ = std::fs::remove_file(&path);
                        continue;
                    }
                    return Err(format!(
                        "[{}] another AutoUpload run is using this channel (started {} min ago). If none is running, delete {}",
                        ch.name,
                        age.as_secs() / 60,
                        path.display()
                    ));
                }
                Err(e) => {
                    return Err(format!(
                        "[{}] could not take the run lock {}: {e}",
                        ch.name,
                        path.display()
                    ))
                }
            }
        }
        Err(format!(
            "[{}] could not take the run lock {}",
            ch.name,
            path.display()
        ))
    }
}

impl Drop for ChannelLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn resolve(base: &Path, p: &str) -> PathBuf {
    let pb = PathBuf::from(p);
    if pb.is_absolute() {
        pb
    } else {
        base.join(pb)
    }
}

/// Merges the channel defaults with the video's sheet. Without a fixed date the video stays private until `when`.
pub fn build_manifest(
    ch: &Channel,
    item: &QueueItem,
    when: Option<DateTime<Utc>>,
) -> Result<Manifest, String> {
    let stem = item
        .video
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let sheet_path = item
        .sheet
        .as_ref()
        .ok_or_else(|| format!("missing sheet {stem}.yaml with the title"))?;
    let f = read_sheet(sheet_path)?;
    let sheet_name = sheet_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let title = f
        .title
        .clone()
        .ok_or_else(|| format!("{sheet_name}: missing 'title'"))?;
    let mut description = f
        .description
        .clone()
        .unwrap_or_default()
        .trim_end()
        .to_string();
    if !ch.cfg.description_footer.is_empty() {
        description = format!("{description}\n\n{}", ch.cfg.description_footer)
            .trim()
            .to_string();
    }
    let mut tags: Vec<String> = Vec::new();
    for t in ch.cfg.base_tags.iter().chain(f.tags.iter().flatten()) {
        if !tags.contains(t) {
            tags.push(t.clone());
        }
    }
    let default_language = f
        .default_language
        .clone()
        .unwrap_or_else(|| ch.cfg.language.clone());
    let privacy = Privacy::parse(f.privacy.as_deref().unwrap_or("private"))
        .map_err(|e| format!("{sheet_name}: {e}"))?;
    let base = ch.queue();
    let thumbnail = f
        .thumbnail
        .as_deref()
        .map(|t| resolve(&base, t))
        .or_else(|| item.thumbnail.clone());
    let publish_at = match f.publish_at.as_deref() {
        Some(s) => Some(parse_publish_at(s).map_err(|e| format!("{sheet_name}: {e}"))?),
        None if privacy == Privacy::Private => when,
        None => None,
    };
    let m = Manifest {
        id: item.id(),
        video: item.video.clone(),
        thumbnail,
        title,
        description,
        tags,
        category_id: f
            .category_id
            .clone()
            .unwrap_or_else(|| ch.cfg.category.clone()),
        default_audio_language: f
            .default_audio_language
            .clone()
            .unwrap_or_else(|| default_language.clone()),
        default_language,
        privacy,
        publish_at,
        made_for_kids: f.made_for_kids.unwrap_or(ch.cfg.made_for_kids),
        contains_synthetic_media: f.contains_synthetic_media.or(ch.cfg.contains_ai),
        notify_subscribers: f.notify_subscribers.unwrap_or(ch.cfg.notify_subscribers),
        localizations: f.localizations.clone().unwrap_or_default(),
        captions: f
            .captions
            .iter()
            .flatten()
            .map(|c| Caption {
                language: c.language.clone(),
                name: c.name.clone(),
                path: resolve(&base, &c.path),
            })
            .collect(),
        playlist_id: f.playlist_id.clone().or_else(|| ch.cfg.playlist.clone()),
    };
    m.validate(Utc::now())
        .map_err(|e| format!("{sheet_name}: {e}"))?;
    Ok(m)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::channel::{create_channel, load_channel};

    pub fn channel(dir: &Path) -> Channel {
        create_channel("test", dir).unwrap();
        load_channel("test", dir).unwrap()
    }

    pub fn put(ch: &Channel, name: &str, sheet: Option<&str>, thumb: bool) {
        let q = ch.queue();
        std::fs::write(
            q.join(format!("{name}.mp4")),
            [
                b"\x00\x00\x00\x18ftypmp42".as_slice(),
                name.as_bytes(),
                &[0u8; 512],
            ]
            .concat(),
        )
        .unwrap();
        if let Some(s) = sheet {
            std::fs::write(q.join(format!("{name}.yaml")), s).unwrap();
        }
        if thumb {
            std::fs::write(
                q.join(format!("{name}.png")),
                [b"\x89PNG\r\n\x1a\n".as_slice(), &[0u8; 64]].concat(),
            )
            .unwrap();
        }
    }

    #[test]
    fn ids() {
        let i = |n: &str| {
            QueueItem {
                video: PathBuf::from(format!("/q/{n}")),
                sheet: None,
                thumbnail: None,
            }
            .id()
        };
        assert_eq!(i("My Video 1.mp4"), "my-video-1");
        assert_eq!(i("Canción Ñandú.mp4"), "cancion-nandu");
        assert_eq!(i("!.mp4"), "video-x");
    }

    #[test]
    fn sheet_and_channel_defaults() {
        let d = tempfile::tempdir().unwrap();
        let mut c = channel(d.path());
        c.cfg.base_tags = vec!["channel".into()];
        c.cfg.description_footer = "Subscribe".into();
        c.cfg.contains_ai = Some(true);
        put(
            &c,
            "My Video 1",
            Some("title: Hello\ndescription: Text\ntags: [one, channel]\n"),
            true,
        );
        let items = queue_items(&c);
        let when = Utc::now() + chrono::Duration::days(3);
        let m = build_manifest(&c, &items[0], Some(when)).unwrap();
        assert_eq!(m.id, "my-video-1");
        assert_eq!(m.tags, vec!["channel", "one"]);
        assert!(m.description.ends_with("Subscribe"));
        assert_eq!(m.contains_synthetic_media, Some(true));
        assert_eq!(m.publish_at, Some(when));
        assert!(m.thumbnail.unwrap().ends_with("My Video 1.png"));
    }

    #[test]
    fn missing_sheet_and_unknown_key() {
        let d = tempfile::tempdir().unwrap();
        let c = channel(d.path());
        put(&c, "a", None, false);
        put(&c, "b", Some("titulo: B\n"), false);
        let items = queue_items(&c);
        assert!(build_manifest(&c, &items[0], None)
            .unwrap_err()
            .contains("missing sheet"));
        assert!(build_manifest(&c, &items[1], None)
            .unwrap_err()
            .contains("titulo"));
    }

    #[test]
    fn lock_blocks_second_run_and_is_released() {
        let d = tempfile::tempdir().unwrap();
        let c = channel(d.path());
        let l = ChannelLock::acquire(&c).unwrap();
        assert!(ChannelLock::acquire(&c)
            .unwrap_err()
            .contains("another AutoUpload run"));
        drop(l);
        assert!(!c.folder.join("run.lock").exists());
        ChannelLock::acquire(&c).unwrap();
    }

    #[test]
    fn settling() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("v.mp4");
        std::fs::write(&p, b"x").unwrap();
        assert!(is_settling(&p, Duration::from_secs(120)));
        assert!(!is_settling(&p, Duration::from_secs(0)));
    }
}
