//! YouTube Data API v3 over REST. Every remote call goes through the [`Api`] trait, so tests use a fake.
//!
//! Uploads use Google's resumable protocol directly: the session URI is reported as soon as Google issues it,
//! and passing it back continues from the last byte Google acknowledged, even from another process.

use reqwest::blocking::{Client, RequestBuilder, Response};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::fmt;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Duration;

use crate::auth::Session;

const API: &str = "https://www.googleapis.com/youtube/v3";
const UPLOAD: &str = "https://www.googleapis.com/upload/youtube/v3";
pub const CHUNK: u64 = 8 * 1024 * 1024; // multiple of 256 KiB, as the protocol requires
pub const MAX_RETRIES: u32 = 8;

/// Daily limits: retrying today is useless, the whole run must stop.
pub const STOP_REASONS: &[&str] = &["quotaExceeded", "dailyLimitExceeded", "uploadLimitExceeded"];
pub const RETRY_REASONS: &[&str] = &[
    "rateLimitExceeded",
    "userRateLimitExceeded",
    "backendError",
    "internalError",
];
const RETRY_STATUS: &[u16] = &[429, 500, 502, 503, 504];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Http,
    Network,
    Auth,
    SessionExpired,
    Local,
}

#[derive(Debug, Clone)]
pub struct ApiError {
    pub kind: ErrorKind,
    pub status: Option<u16>,
    pub reason: Option<String>,
    pub message: String,
    pub retry_after: Option<f64>,
}

impl ApiError {
    pub fn local(msg: impl Into<String>) -> ApiError {
        ApiError {
            kind: ErrorKind::Local,
            status: None,
            reason: None,
            message: msg.into(),
            retry_after: None,
        }
    }
    pub fn auth(msg: impl Into<String>) -> ApiError {
        ApiError {
            kind: ErrorKind::Auth,
            status: None,
            reason: None,
            message: msg.into(),
            retry_after: None,
        }
    }
    pub fn http(status: u16, reason: Option<&str>, message: impl Into<String>) -> ApiError {
        ApiError {
            kind: ErrorKind::Http,
            status: Some(status),
            reason: reason.map(String::from),
            message: message.into(),
            retry_after: None,
        }
    }
    pub fn network(msg: impl Into<String>) -> ApiError {
        ApiError {
            kind: ErrorKind::Network,
            status: None,
            reason: None,
            message: msg.into(),
            retry_after: None,
        }
    }

    pub fn is_stop(&self) -> bool {
        self.reason
            .as_deref()
            .map(|r| STOP_REASONS.contains(&r))
            .unwrap_or(false)
    }

    pub fn is_retriable(&self) -> bool {
        if self.is_stop() {
            return false;
        }
        match self.kind {
            ErrorKind::Network => true,
            ErrorKind::Http => {
                let s = self.status.unwrap_or(0);
                RETRY_STATUS.contains(&s)
                    || (s == 403
                        && self
                            .reason
                            .as_deref()
                            .map(|r| RETRY_REASONS.contains(&r))
                            .unwrap_or(false))
            }
            _ => false,
        }
    }

    fn from_response(resp: Response) -> ApiError {
        let status = resp.status().as_u16();
        let retry_after = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.trim().parse::<f64>().ok());
        let text = resp.text().unwrap_or_default();
        let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        let reason = v["error"]["errors"][0]["reason"].as_str().map(String::from);
        let message = v["error"]["message"]
            .as_str()
            .map(String::from)
            .unwrap_or_else(|| {
                let t: String = text.chars().take(300).collect();
                if t.is_empty() {
                    format!("HTTP {status}")
                } else {
                    t
                }
            });
        ApiError {
            kind: ErrorKind::Http,
            status: Some(status),
            reason,
            message,
            retry_after,
        }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.status, &self.reason) {
            (Some(s), Some(r)) => write!(f, "{} (HTTP {s}, {r})", self.message),
            (Some(s), None) => write!(f, "{} (HTTP {s})", self.message),
            _ => write!(f, "{}", self.message),
        }
    }
}

impl std::error::Error for ApiError {}

pub fn backoff(attempt: u32, e: &ApiError) -> Duration {
    if let Some(s) = e.retry_after {
        return Duration::from_secs_f64(s.clamp(0.0, 300.0));
    }
    let jitter: f64 = rand::random();
    Duration::from_secs_f64((2f64.powi(attempt as i32) + jitter).min(64.0))
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ChannelInfo {
    pub id: String,
    pub title: String,
}

pub trait Api {
    fn my_channel(&self) -> Result<ChannelInfo, ApiError>;
    /// Latest channel uploads (also private and scheduled) with the given parts.
    fn uploads(&self, max: usize, parts: &str) -> Result<Vec<Value>, ApiError>;
    fn get_video(&self, id: &str) -> Result<Option<Value>, ApiError>;
    /// Uploads (or resumes, with `session`) and returns the video ID.
    #[allow(clippy::too_many_arguments)]
    fn insert_video(
        &self,
        body: &Value,
        part: &str,
        path: &Path,
        notify: bool,
        session: Option<&str>,
        on_session: &mut dyn FnMut(&str),
        on_progress: &mut dyn FnMut(f32),
    ) -> Result<String, ApiError>;
    fn set_thumbnail(&self, video_id: &str, path: &Path) -> Result<(), ApiError>;
    fn insert_caption(
        &self,
        video_id: &str,
        language: &str,
        name: &str,
        path: &Path,
    ) -> Result<(), ApiError>;
    fn add_to_playlist(&self, playlist_id: &str, video_id: &str) -> Result<(), ApiError>;

    /// ID of a recent upload made from this same file (fileDetails is visible only to the owner), or None.
    /// Used when an upload may have reached YouTube but its response was lost, so it is not uploaded twice.
    fn find_by_file(&self, file_name: &str, file_size: u64) -> Result<Option<String>, ApiError> {
        for v in self.uploads(50, "fileDetails")? {
            let fd = &v["fileDetails"];
            let size = fd["fileSize"]
                .as_str()
                .and_then(|s| s.parse::<u64>().ok())
                .or_else(|| fd["fileSize"].as_u64());
            if fd["fileName"].as_str() == Some(file_name) && size == Some(file_size) {
                return Ok(v["id"].as_str().map(String::from));
            }
        }
        Ok(None)
    }
}

/// The real client, signed in as one profile.
pub struct YouTube {
    http: Client,
    session: RefCell<Session>,
    sleep: fn(Duration),
}

enum Status {
    Done(String),
    Partial(u64),
}

impl YouTube {
    pub fn new(session: Session) -> YouTube {
        let http = Client::builder()
            .timeout(Duration::from_secs(600))
            .connect_timeout(Duration::from_secs(30))
            .user_agent(format!("AutoUpload/{}", crate::VERSION))
            .build()
            .expect("http client");
        YouTube {
            http,
            session: RefCell::new(session),
            sleep: std::thread::sleep,
        }
    }

    fn bearer(&self) -> Result<String, ApiError> {
        self.session
            .borrow_mut()
            .bearer()
            .map_err(|e| ApiError::auth(e.0))
    }

    /// Sends a request, retrying recoverable errors (5xx, 429, short-term rate limits, network).
    fn send(&self, build: &dyn Fn(&Client, &str) -> RequestBuilder) -> Result<Response, ApiError> {
        let mut attempt = 0;
        let mut refreshed = false;
        loop {
            let token = self.bearer()?;
            let e = match build(&self.http, &token).send() {
                Ok(r) if r.status().is_success() => return Ok(r),
                Ok(r) if r.status().as_u16() == 401 && !refreshed => {
                    refreshed = true;
                    self.session
                        .borrow_mut()
                        .refresh()
                        .map_err(|e| ApiError::auth(e.0))?;
                    continue;
                }
                Ok(r) => ApiError::from_response(r),
                Err(e) => ApiError::network(e.to_string()),
            };
            if !e.is_retriable() || attempt >= MAX_RETRIES {
                return Err(e);
            }
            attempt += 1;
            (self.sleep)(backoff(attempt, &e));
        }
    }

    fn get_json(&self, url: &str, query: &[(&str, &str)]) -> Result<Value, ApiError> {
        let r = self.send(&|c, t| c.get(url).bearer_auth(t).query(query))?;
        r.json().map_err(|e| ApiError::network(e.to_string()))
    }

    fn start_session(
        &self,
        body: &Value,
        part: &str,
        size: u64,
        notify: bool,
    ) -> Result<String, ApiError> {
        let url = format!("{UPLOAD}/videos");
        let notify = if notify { "true" } else { "false" };
        let r = self.send(&|c, t| {
            c.post(&url)
                .bearer_auth(t)
                .query(&[
                    ("uploadType", "resumable"),
                    ("part", part),
                    ("notifySubscribers", notify),
                ])
                .header("X-Upload-Content-Length", size.to_string())
                .header("X-Upload-Content-Type", "video/*")
                .json(body)
        })?;
        r.headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .map(String::from)
            .ok_or_else(|| ApiError::local("Google did not return an upload session"))
    }

    fn parse_range(resp: &Response) -> u64 {
        resp.headers()
            .get("range")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.rsplit('-').next())
            .and_then(|n| n.trim().parse::<u64>().ok())
            .map(|last| last + 1)
            .unwrap_or(0)
    }

    fn finished(resp: Response) -> Result<Status, ApiError> {
        let v: Value = resp.json().map_err(|e| ApiError::network(e.to_string()))?;
        v["id"]
            .as_str()
            .map(|s| Status::Done(s.to_string()))
            .ok_or_else(|| ApiError::local(format!("unexpected response without id: {v}")))
    }

    /// Asks Google how much of the upload it already has.
    fn query_status(&self, uri: &str, size: u64) -> Result<Status, ApiError> {
        let token = self.bearer()?;
        let r = self
            .http
            .put(uri)
            .bearer_auth(token)
            .header("Content-Range", format!("bytes */{size}"))
            .header("Content-Length", "0")
            .send()
            .map_err(|e| ApiError::network(e.to_string()))?;
        match r.status().as_u16() {
            200 | 201 => Self::finished(r),
            308 => Ok(Status::Partial(Self::parse_range(&r))),
            404 | 410 => Err(ApiError {
                kind: ErrorKind::SessionExpired,
                ..ApiError::from_response(r)
            }),
            _ => Err(ApiError::from_response(r)),
        }
    }
}

impl Api for YouTube {
    fn my_channel(&self) -> Result<ChannelInfo, ApiError> {
        let v = self.get_json(
            &format!("{API}/channels"),
            &[("part", "id,snippet"), ("mine", "true")],
        )?;
        let item = v["items"]
            .get(0)
            .ok_or_else(|| ApiError::local("the authenticated account has no YouTube channel"))?;
        Ok(ChannelInfo {
            id: item["id"].as_str().unwrap_or_default().to_string(),
            title: item["snippet"]["title"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        })
    }

    fn uploads(&self, max: usize, parts: &str) -> Result<Vec<Value>, ApiError> {
        let ch = self.get_json(
            &format!("{API}/channels"),
            &[("part", "contentDetails"), ("mine", "true")],
        )?;
        let Some(playlist) = ch["items"][0]["contentDetails"]["relatedPlaylists"]["uploads"]
            .as_str()
            .map(String::from)
        else {
            return Ok(vec![]);
        };
        let mut ids: Vec<String> = Vec::new();
        let mut page: Option<String> = None;
        while ids.len() < max {
            let mut q = vec![
                ("part", "contentDetails"),
                ("playlistId", playlist.as_str()),
                ("maxResults", "50"),
            ];
            if let Some(p) = page.as_deref() {
                q.push(("pageToken", p));
            }
            let r = self.get_json(&format!("{API}/playlistItems"), &q)?;
            for it in r["items"].as_array().into_iter().flatten() {
                if let Some(id) = it["contentDetails"]["videoId"].as_str() {
                    ids.push(id.to_string());
                }
            }
            page = r["nextPageToken"].as_str().map(String::from);
            if page.is_none() {
                break;
            }
        }
        ids.truncate(max);
        let mut out = Vec::new();
        for chunk in ids.chunks(50) {
            let joined = chunk.join(",");
            let r = self.get_json(
                &format!("{API}/videos"),
                &[
                    ("part", parts),
                    ("id", joined.as_str()),
                    ("maxResults", "50"),
                ],
            )?;
            out.extend(r["items"].as_array().cloned().unwrap_or_default());
        }
        Ok(out)
    }

    fn get_video(&self, id: &str) -> Result<Option<Value>, ApiError> {
        let r = self.get_json(
            &format!("{API}/videos"),
            &[
                ("part", "snippet,status,localizations,processingDetails"),
                ("id", id),
            ],
        )?;
        Ok(r["items"].get(0).cloned())
    }

    fn insert_video(
        &self,
        body: &Value,
        part: &str,
        path: &Path,
        notify: bool,
        session: Option<&str>,
        on_session: &mut dyn FnMut(&str),
        on_progress: &mut dyn FnMut(f32),
    ) -> Result<String, ApiError> {
        let size = std::fs::metadata(path)
            .map_err(|e| ApiError::local(format!("{}: {e}", path.display())))?
            .len();
        let (uri, mut offset) = match session {
            Some(uri) => match self.query_status(uri, size)? {
                Status::Done(id) => {
                    on_progress(1.0);
                    return Ok(id);
                }
                Status::Partial(n) => (uri.to_string(), n),
            },
            None => {
                let uri = self.start_session(body, part, size, notify)?;
                on_session(&uri);
                (uri, 0)
            }
        };
        let mut file = std::fs::File::open(path)
            .map_err(|e| ApiError::local(format!("{}: {e}", path.display())))?;
        let mut retries = 0;
        loop {
            let end = (offset + CHUNK).min(size);
            let mut buf = vec![0u8; (end - offset) as usize];
            file.seek(SeekFrom::Start(offset))
                .and_then(|_| file.read_exact(&mut buf))
                .map_err(|e| ApiError::local(e.to_string()))?;
            let token = self.bearer()?;
            let last = if end == 0 { 0 } else { end - 1 };
            let sent = self
                .http
                .put(&uri)
                .bearer_auth(token)
                .header("Content-Range", format!("bytes {offset}-{last}/{size}"))
                .body(buf)
                .send();
            let e = match sent {
                Ok(r) => match r.status().as_u16() {
                    200 | 201 => {
                        if let Status::Done(id) = Self::finished(r)? {
                            on_progress(1.0);
                            return Ok(id);
                        }
                        unreachable!()
                    }
                    308 => {
                        offset = Self::parse_range(&r);
                        retries = 0;
                        on_progress(if size == 0 {
                            1.0
                        } else {
                            offset as f32 / size as f32
                        });
                        continue;
                    }
                    _ => ApiError::from_response(r),
                },
                Err(err) => ApiError::network(err.to_string()),
            };
            if !e.is_retriable() || retries >= MAX_RETRIES {
                return Err(e);
            }
            retries += 1;
            (self.sleep)(backoff(retries, &e));
            match self.query_status(&uri, size) {
                Ok(Status::Done(id)) => {
                    on_progress(1.0);
                    return Ok(id);
                }
                Ok(Status::Partial(n)) => offset = n,
                Err(q) if q.kind == ErrorKind::SessionExpired => return Err(q),
                Err(_) => {} // keep the offset and try the chunk again
            }
        }
    }

    fn set_thumbnail(&self, video_id: &str, path: &Path) -> Result<(), ApiError> {
        let bytes =
            std::fs::read(path).map_err(|e| ApiError::local(format!("{}: {e}", path.display())))?;
        let mime = if crate::validate::ext_lower(path) == "png" {
            "image/png"
        } else {
            "image/jpeg"
        };
        let url = format!("{UPLOAD}/thumbnails/set");
        self.send(&|c, t| {
            c.post(&url)
                .bearer_auth(t)
                .query(&[("videoId", video_id), ("uploadType", "media")])
                .header("Content-Type", mime)
                .body(bytes.clone())
        })?;
        Ok(())
    }

    fn insert_caption(
        &self,
        video_id: &str,
        language: &str,
        name: &str,
        path: &Path,
    ) -> Result<(), ApiError> {
        let media =
            std::fs::read(path).map_err(|e| ApiError::local(format!("{}: {e}", path.display())))?;
        let meta = json!({"snippet": {"videoId": video_id, "language": language, "name": name, "isDraft": false}});
        let boundary = format!("autoupload{}", rand::random::<u64>());
        let mut body = Vec::with_capacity(media.len() + 512);
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{meta}\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(
            format!("--{boundary}\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes(),
        );
        body.extend_from_slice(&media);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let url = format!("{UPLOAD}/captions");
        let ctype = format!("multipart/related; boundary={boundary}");
        self.send(&|c, t| {
            c.post(&url)
                .bearer_auth(t)
                .query(&[("uploadType", "multipart"), ("part", "snippet")])
                .header("Content-Type", ctype.as_str())
                .body(body.clone())
        })?;
        Ok(())
    }

    fn add_to_playlist(&self, playlist_id: &str, video_id: &str) -> Result<(), ApiError> {
        let body = json!({"snippet": {"playlistId": playlist_id, "resourceId": {"kind": "youtube#video", "videoId": video_id}}});
        let url = format!("{API}/playlistItems");
        self.send(&|c, t| {
            c.post(&url)
                .bearer_auth(t)
                .query(&[("part", "snippet")])
                .json(&body)
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification() {
        assert!(ApiError::http(403, Some("quotaExceeded"), "x").is_stop());
        assert!(!ApiError::http(403, Some("quotaExceeded"), "x").is_retriable());
        assert!(ApiError::http(403, Some("rateLimitExceeded"), "x").is_retriable());
        assert!(!ApiError::http(403, Some("forbidden"), "x").is_retriable());
        for s in [429, 500, 503] {
            assert!(ApiError::http(s, None, "x").is_retriable());
        }
        assert!(!ApiError::http(400, None, "x").is_retriable());
        assert!(ApiError::network("reset").is_retriable());
        assert!(!ApiError::auth("expired").is_retriable());
    }

    #[test]
    fn retry_after_wins() {
        let mut e = ApiError::http(429, None, "x");
        e.retry_after = Some(7.0);
        assert_eq!(backoff(1, &e), Duration::from_secs(7));
        assert!(backoff(10, &ApiError::http(503, None, "x")) <= Duration::from_secs(64));
    }
}
