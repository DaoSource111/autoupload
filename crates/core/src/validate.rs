//! Local checks (no network): files, formats, durations and captions.

use regex::Regex;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

use crate::manifest::Manifest;

pub const VIDEO_EXT: &[&str] = &["mp4", "mov", "mkv", "webm", "avi", "m4v"];
pub const THUMB_EXT: &[&str] = &["jpg", "jpeg", "png"];
const THUMB_MAX: u64 = 50 * 1024 * 1024; // thumbnails.set: 50 MB
const CAPTION_MAX: u64 = 100 * 1024 * 1024; // captions.insert: 100 MB

#[derive(Debug, Default, Clone)]
pub struct Report {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
}

pub fn ext_lower(p: &Path) -> String {
    p.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

pub fn find_ffprobe() -> Option<String> {
    let exe = if cfg!(windows) {
        "ffprobe.exe"
    } else {
        "ffprobe"
    };
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(exe))
        .find(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
}

/// Duration in seconds via ffprobe, or None if ffprobe is missing or fails.
pub fn probe_duration(path: &Path, ffprobe: Option<&str>) -> Option<f64> {
    let out = Command::new(ffprobe?)
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .ok()?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    v["format"]["duration"].as_str()?.parse().ok()
}

fn cue_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(\d{2}):(\d{2}):(\d{2})[,.](\d{3})\s*-->\s*(\d{2}):(\d{2}):(\d{2})[,.](\d{3})")
            .expect("cue regex")
    })
}

/// Errors in an SRT/VTT: header, reversed times, overlaps and cues outside the video.
pub fn validate_caption_text(text: &str, ext: &str, video_duration: Option<f64>) -> Vec<String> {
    let mut errors = Vec::new();
    if ext == "vtt" && !text.trim_start_matches('\u{feff}').starts_with("WEBVTT") {
        errors.push("VTT is missing the WEBVTT header".to_string());
    }
    let secs = |c: &regex::Captures, i: usize| -> f64 {
        let n = |k: usize| c[k].parse::<f64>().unwrap_or(0.0);
        n(i) * 3600.0 + n(i + 1) * 60.0 + n(i + 2) + n(i + 3) / 1000.0
    };
    let cues: Vec<(f64, f64)> = cue_re()
        .captures_iter(text)
        .map(|c| (secs(&c, 1), secs(&c, 5)))
        .collect();
    if cues.is_empty() {
        errors.push("contains no cue with timings".to_string());
    }
    let mut prev_end = 0.0f64;
    for (i, (a, b)) in cues.iter().enumerate() {
        let n = i + 1;
        if b <= a {
            errors.push(format!("cue {n}: end is not after start"));
        }
        if a + 1e-3 < prev_end {
            errors.push(format!("cue {n}: overlaps the previous one"));
        }
        if let Some(d) = video_duration {
            if *b > d + 0.5 {
                errors.push(format!("cue {n}: ends after the video ({b:.2}s > {d:.2}s)"));
            }
        }
        prev_end = prev_end.max(*b);
    }
    errors
}

fn is_image(path: &Path) -> bool {
    let mut head = [0u8; 8];
    let n = std::fs::File::open(path)
        .and_then(|mut f| std::io::Read::read(&mut f, &mut head))
        .unwrap_or(0);
    let h = &head[..n];
    h.starts_with(b"\x89PNG\r\n\x1a\n") || h.starts_with(&[0xFF, 0xD8, 0xFF])
}

pub fn validate_files(m: &Manifest) -> Report {
    let mut rep = Report::default();
    let ffprobe = find_ffprobe();
    let mut video_duration = None;
    if !m.video.is_file() {
        rep.errors
            .push(format!("video file not found: {}", m.video.display()));
    } else {
        if !VIDEO_EXT.contains(&ext_lower(&m.video).as_str()) {
            rep.warnings
                .push(format!("unusual video extension: {}", ext_lower(&m.video)));
        }
        video_duration = probe_duration(&m.video, ffprobe.as_deref());
        if ffprobe.is_none() {
            rep.warnings
                .push("no ffprobe: video duration was not checked".into());
        }
    }
    if let Some(t) = &m.thumbnail {
        if !t.is_file() {
            rep.errors
                .push(format!("thumbnail file not found: {}", t.display()));
        } else if !THUMB_EXT.contains(&ext_lower(t).as_str()) {
            rep.errors.push("thumbnail must be JPG or PNG".into());
        } else if std::fs::metadata(t).map(|md| md.len()).unwrap_or(0) > THUMB_MAX {
            rep.errors.push("thumbnail is over 50 MB".into());
        } else if !is_image(t) {
            rep.errors
                .push("thumbnail is not a valid JPEG/PNG image".into());
        }
    }
    for c in &m.captions {
        if !c.path.is_file() {
            rep.errors.push(format!(
                "caption file not found [{}]: {}",
                c.language,
                c.path.display()
            ));
            continue;
        }
        if std::fs::metadata(&c.path).map(|md| md.len()).unwrap_or(0) > CAPTION_MAX {
            rep.errors
                .push(format!("caption [{}] is over 100 MB", c.language));
            continue;
        }
        let ext = ext_lower(&c.path);
        if ext != "srt" && ext != "vtt" {
            rep.warnings.push(format!(
                "caption [{}] in .{ext}: only SRT and VTT are validated",
                c.language
            ));
            continue;
        }
        let text = std::fs::read_to_string(&c.path).unwrap_or_default();
        for e in validate_caption_text(&text, &ext, video_duration) {
            rep.errors.push(format!("caption [{}]: {e}", c.language));
        }
    }
    rep
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captions() {
        let srt =
            "1\n00:00:00,000 --> 00:00:02,000\nHello\n\n2\n00:00:02,500 --> 00:00:04,000\nBye\n";
        assert!(validate_caption_text(srt, "srt", Some(5.0)).is_empty());
        assert!(validate_caption_text(srt, "vtt", None)
            .iter()
            .any(|e| e.contains("WEBVTT")));
        assert!(validate_caption_text(srt, "srt", Some(3.0))
            .iter()
            .any(|e| e.contains("ends after")));
        let bad = "1\n00:00:02,000 --> 00:00:01,000\nx\n";
        assert!(validate_caption_text(bad, "srt", None)
            .iter()
            .any(|e| e.contains("not after")));
        assert!(validate_caption_text("nothing", "srt", None)
            .iter()
            .any(|e| e.contains("no cue")));
    }
}
