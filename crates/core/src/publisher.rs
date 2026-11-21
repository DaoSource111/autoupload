//! Staged publishing: validate → upload (or resume) → thumbnail → captions → playlist → verify → report.
//! Every stage is recorded in SQLite; an interrupted run resumes where it stopped without uploading again.

use chrono::Utc;
use serde::Serialize;
use std::path::Path;

use crate::manifest::{Manifest, Privacy};
use crate::state::{file_sha256, Record, State};
use crate::validate::validate_files;
use crate::youtube::{Api, ApiError, ChannelInfo, ErrorKind};

#[derive(Debug, Clone, Serialize)]
pub struct Verification {
    pub ok: bool,
    pub problems: Vec<String>,
    pub upload_status: Option<String>,
    pub processing: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublishResult {
    pub manifest_id: String,
    pub channel: ChannelInfo,
    pub video_id: String,
    pub resumed: bool,
    pub previous_manifest_id: Option<String>,
    pub steps: Vec<String>,
    pub warnings: Vec<String>,
    pub verification: Verification,
    pub url: String,
    pub studio_url: String,
    pub generated: String,
}

#[derive(Debug, Clone)]
pub enum PublishError {
    /// Blocked before touching YouTube (validation, missing --yes or --allow-public).
    Safety(String),
    Api(ApiError),
    Local(String),
}

impl std::fmt::Display for PublishError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PublishError::Safety(s) | PublishError::Local(s) => write!(f, "{s}"),
            PublishError::Api(e) => write!(f, "{e}"),
        }
    }
}

impl From<ApiError> for PublishError {
    fn from(e: ApiError) -> Self {
        PublishError::Api(e)
    }
}

impl From<rusqlite::Error> for PublishError {
    fn from(e: rusqlite::Error) -> Self {
        PublishError::Local(format!("state database: {e}"))
    }
}

fn find_existing(m: &Manifest, api: &dyn Api) -> Result<Option<String>, PublishError> {
    let name = m
        .video
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let size = std::fs::metadata(&m.video).map(|md| md.len()).unwrap_or(0);
    Ok(api.find_by_file(&name, size)?)
}

/// Uploads the video once: resumes a saved session, or finds an upload whose response was lost.
fn upload(
    m: &Manifest,
    api: &dyn Api,
    state: &State,
    sha: &str,
    channel_id: &str,
    prev: Option<&Record>,
    progress: &mut dyn FnMut(f32),
) -> Result<(String, String), PublishError> {
    let session = prev.and_then(|p| p.session.clone());
    let started_before = prev.map(|p| p.has("upload_started")).unwrap_or(false);
    if started_before && session.is_none() {
        // An earlier run started uploading but never learned the result: the video may already be on the channel.
        if let Some(id) = find_existing(m, api)? {
            return Ok((
                id,
                "videos.insert (skipped: found on the channel from an earlier run)".into(),
            ));
        }
    }
    state.record(&m.id, sha, channel_id, None, Some("upload_started"), None)?;
    let (body, part) = m.insert_body();
    let mut keep = |uri: &str| {
        let _ = state.record(&m.id, sha, channel_id, None, None, Some(uri));
    };
    if let Some(s) = session.as_deref() {
        match api.insert_video(
            &body,
            &part,
            &m.video,
            m.notify_subscribers,
            Some(s),
            &mut keep,
            progress,
        ) {
            Ok(id) => return Ok((id, "videos.insert (resumed)".into())),
            Err(e) if e.kind == ErrorKind::SessionExpired => {
                state.clear_session(sha, channel_id)?;
                if let Some(id) = find_existing(m, api)? {
                    return Ok((
                        id,
                        "videos.insert (skipped: found on the channel from an earlier run)".into(),
                    ));
                }
            }
            Err(e) => return Err(e.into()),
        }
    }
    let id = api.insert_video(
        &body,
        &part,
        &m.video,
        m.notify_subscribers,
        None,
        &mut keep,
        progress,
    )?;
    Ok((id, "videos.insert".into()))
}

/// Runs the plan against YouTube. Requires `confirm`; `public` also requires `allow_public`.
pub fn publish(
    m: &Manifest,
    api: &dyn Api,
    state: &State,
    confirm: bool,
    allow_public: bool,
    progress: &mut dyn FnMut(f32),
) -> Result<PublishResult, PublishError> {
    let rep = validate_files(m);
    if !rep.ok() {
        return Err(PublishError::Safety(format!(
            "the video did not pass validation:\n- {}",
            rep.errors.join("\n- ")
        )));
    }
    if !confirm {
        return Err(PublishError::Safety(
            "explicit confirmation (--yes) is required to modify real content".into(),
        ));
    }
    if m.privacy == Privacy::Public && !allow_public {
        return Err(PublishError::Safety(
            "the sheet asks for `public`: add --allow-public if you really want to publish it"
                .into(),
        ));
    }
    let channel = api.my_channel()?;
    let sha = file_sha256(&m.video)
        .map_err(|e| PublishError::Local(format!("{}: {e}", m.video.display())))?;
    let prev = state.find(&sha, &channel.id)?;
    let stages: Vec<String> = prev.as_ref().map(|p| p.stages.clone()).unwrap_or_default();
    let has = |s: &str| stages.iter().any(|x| x == s);
    let mut steps = Vec::new();
    let mut warnings = Vec::new();
    let previous_manifest_id = prev.as_ref().map(|p| p.manifest_id.clone());

    state.record(&m.id, &sha, &channel.id, None, Some("validated"), None)?;
    let video_id = if has("uploaded") {
        steps.push("videos.insert (skipped: already uploaded)".to_string());
        prev.as_ref()
            .and_then(|p| p.video_id.clone())
            .ok_or_else(|| PublishError::Local("state says uploaded but has no video ID".into()))?
    } else {
        let (id, how) = upload(m, api, state, &sha, &channel.id, prev.as_ref(), progress)?;
        state.record(&m.id, &sha, &channel.id, Some(&id), Some("uploaded"), None)?;
        state.event(&m.id, "uploaded", &id)?;
        steps.push(how);
        id
    };

    if let Some(t) = &m.thumbnail {
        if !has("thumbnail") {
            match api.set_thumbnail(&video_id, t) {
                Ok(()) => {
                    state.record(&m.id, &sha, &channel.id, None, Some("thumbnail"), None)?;
                    steps.push("thumbnails.set".into());
                }
                Err(e) if e.status == Some(403) => {
                    // Custom thumbnails need a verified channel; the video itself is fine, so this does not fail it.
                    warnings.push(
                        "thumbnail rejected by YouTube (403): custom thumbnails need a verified channel, \
                         see https://www.youtube.com/verify; set it later in YouTube Studio"
                            .into(),
                    );
                    steps.push("thumbnails.set (rejected: channel not verified)".into());
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    if !m.captions.is_empty() && !has("captions") {
        for c in &m.captions {
            match api.insert_caption(&video_id, &c.language, &c.name, &c.path) {
                Ok(()) => steps.push(format!("captions.insert[{}]", c.language)),
                Err(e) if e.status == Some(409) => {
                    steps.push(format!("captions.insert[{}] (already existed)", c.language))
                }
                Err(e) => return Err(e.into()),
            }
        }
        state.record(&m.id, &sha, &channel.id, None, Some("captions"), None)?;
    }

    if let Some(pl) = &m.playlist_id {
        if !has("playlist") {
            api.add_to_playlist(pl, &video_id)?;
            state.record(&m.id, &sha, &channel.id, None, Some("playlist"), None)?;
            steps.push("playlistItems.insert".into());
        }
    }

    let verification = verify(m, api, &video_id)?;
    if verification.ok {
        state.record(&m.id, &sha, &channel.id, None, Some("verified"), None)?;
    }
    Ok(PublishResult {
        manifest_id: m.id.clone(),
        url: format!("https://www.youtube.com/watch?v={video_id}"),
        studio_url: format!("https://studio.youtube.com/video/{video_id}/edit"),
        resumed: prev.is_some(),
        channel,
        video_id,
        previous_manifest_id,
        steps,
        warnings,
        verification,
        generated: Utc::now().format("%Y-%m-%dT%H:%M:%S+00:00").to_string(),
    })
}

pub fn verify(m: &Manifest, api: &dyn Api, video_id: &str) -> Result<Verification, PublishError> {
    let Some(v) = api.get_video(video_id)? else {
        return Ok(Verification {
            ok: false,
            problems: vec!["videos.list did not return the video".into()],
            upload_status: None,
            processing: None,
        });
    };
    let mut problems = Vec::new();
    let privacy = v["status"]["privacyStatus"].as_str().unwrap_or("");
    if privacy != m.privacy.as_str() {
        problems.push(format!("privacy {privacy} ≠ {}", m.privacy.as_str()));
    }
    if v["snippet"]["title"].as_str() != Some(m.title.as_str()) {
        problems.push("title does not match".into());
    }
    let missing: Vec<&String> = m
        .localizations
        .keys()
        .filter(|k| v["localizations"].get(k.as_str()).is_none())
        .collect();
    if !missing.is_empty() {
        problems.push(format!("missing localizations: {missing:?}"));
    }
    Ok(Verification {
        ok: problems.is_empty(),
        problems,
        upload_status: v["status"]["uploadStatus"].as_str().map(String::from),
        processing: v["processingDetails"]["processingStatus"]
            .as_str()
            .map(String::from),
    })
}

pub fn write_report(result: &PublishResult, out: &Path) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(result).expect("report serialize");
    crate::fsutil::write_atomic(out, &text)
}

#[cfg(test)]
#[allow(clippy::cmp_owned, clippy::too_many_arguments)]
pub(crate) mod fake {
    //! A fake YouTube that records calls and can fail on demand. Nothing touches the network.
    use super::*;
    use serde_json::{json, Value};
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[derive(Default)]
    pub struct FakeApi {
        pub calls: RefCell<Vec<String>>,
        pub fail: RefCell<HashMap<String, Vec<ApiError>>>,
        pub videos: RefCell<Vec<Value>>,
        pub existing: RefCell<Vec<Value>>,
        pub channel_id: String,
        pub sessions_used: RefCell<Vec<Option<String>>>,
    }

    impl FakeApi {
        pub fn new() -> FakeApi {
            FakeApi {
                channel_id: "UC_TEST".into(),
                ..Default::default()
            }
        }
        pub fn failing(self, method: &str, errors: Vec<ApiError>) -> FakeApi {
            self.fail.borrow_mut().insert(method.into(), errors);
            self
        }
        fn maybe_fail(&self, method: &str) -> Result<(), ApiError> {
            let mut f = self.fail.borrow_mut();
            if let Some(q) = f.get_mut(method) {
                if !q.is_empty() {
                    return Err(q.remove(0));
                }
            }
            Ok(())
        }
        pub fn count(&self, call: &str) -> usize {
            self.calls
                .borrow()
                .iter()
                .filter(|c| c.as_str() == call)
                .count()
        }
    }

    impl Api for FakeApi {
        fn my_channel(&self) -> Result<ChannelInfo, ApiError> {
            self.calls.borrow_mut().push("channels.list".into());
            Ok(ChannelInfo {
                id: self.channel_id.clone(),
                title: "Test channel".into(),
            })
        }
        fn uploads(&self, _max: usize, _parts: &str) -> Result<Vec<Value>, ApiError> {
            self.calls.borrow_mut().push("uploads".into());
            let mut all = self.existing.borrow().clone();
            all.extend(self.videos.borrow().iter().cloned());
            Ok(all)
        }
        fn get_video(&self, id: &str) -> Result<Option<Value>, ApiError> {
            self.calls.borrow_mut().push("videos.list".into());
            Ok(self.videos.borrow().iter().find(|v| v["id"] == id).cloned())
        }
        fn insert_video(
            &self,
            body: &Value,
            _part: &str,
            path: &Path,
            _notify: bool,
            session: Option<&str>,
            on_session: &mut dyn FnMut(&str),
            on_progress: &mut dyn FnMut(f32),
        ) -> Result<String, ApiError> {
            self.calls.borrow_mut().push(if session.is_some() {
                "videos.insert:resume".into()
            } else {
                "videos.insert".into()
            });
            self.sessions_used
                .borrow_mut()
                .push(session.map(String::from));
            let n = self.videos.borrow().len() + 1;
            if session.is_none() {
                on_session(&format!("https://upload.example/session/{n}"));
            }
            self.maybe_fail("videos.insert")?;
            let id = format!("VID{n}");
            let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            let mut v = body.clone();
            v["id"] = json!(id);
            v["status"]["uploadStatus"] = json!("uploaded");
            v["fileDetails"] = json!({"fileName": path.file_name().unwrap().to_string_lossy(), "fileSize": size.to_string()});
            self.videos.borrow_mut().push(v);
            on_progress(1.0);
            Ok(id)
        }
        fn set_thumbnail(&self, _video_id: &str, _path: &Path) -> Result<(), ApiError> {
            self.calls.borrow_mut().push("thumbnails.set".into());
            self.maybe_fail("thumbnails.set")
        }
        fn insert_caption(
            &self,
            _video_id: &str,
            language: &str,
            _name: &str,
            _path: &Path,
        ) -> Result<(), ApiError> {
            self.calls
                .borrow_mut()
                .push(format!("captions.insert:{language}"));
            self.maybe_fail("captions.insert")
        }
        fn add_to_playlist(&self, _playlist_id: &str, _video_id: &str) -> Result<(), ApiError> {
            self.calls.borrow_mut().push("playlistItems.insert".into());
            self.maybe_fail("playlistItems.insert")
        }
        fn find_by_file(
            &self,
            file_name: &str,
            file_size: u64,
        ) -> Result<Option<String>, ApiError> {
            self.calls.borrow_mut().push("find_by_file".into());
            Ok(self
                .videos
                .borrow()
                .iter()
                .find(|v| {
                    v["fileDetails"]["fileName"] == file_name
                        && v["fileDetails"]["fileSize"] == file_size.to_string()
                })
                .and_then(|v| v["id"].as_str().map(String::from)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeApi;
    use super::*;
    use crate::manifest::{Caption, Localization};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn setup() -> (tempfile::TempDir, Manifest, State) {
        let d = tempfile::tempdir().unwrap();
        let video = d.path().join("video.mp4");
        std::fs::write(
            &video,
            [b"\x00\x00\x00\x18ftypmp42".as_slice(), &[0u8; 2048]].concat(),
        )
        .unwrap();
        let thumb = d.path().join("thumb.png");
        std::fs::write(
            &thumb,
            [b"\x89PNG\r\n\x1a\n".as_slice(), &[0u8; 64]].concat(),
        )
        .unwrap();
        let srt = d.path().join("en.srt");
        std::fs::write(&srt, "1\n00:00:00,000 --> 00:00:02,000\nHello\n").unwrap();
        let mut locs = BTreeMap::new();
        locs.insert(
            "de".to_string(),
            Localization {
                title: "Hallo".into(),
                description: "Test".into(),
            },
        );
        let m = Manifest {
            id: "ep-test".into(),
            video,
            thumbnail: Some(thumb),
            title: "What if darkness had weight?".into(),
            description: "Test".into(),
            tags: vec!["what if".into()],
            category_id: "27".into(),
            default_language: "en".into(),
            default_audio_language: "en".into(),
            privacy: Privacy::Private,
            publish_at: None,
            made_for_kids: false,
            contains_synthetic_media: None,
            notify_subscribers: false,
            localizations: locs,
            captions: vec![Caption {
                language: "en".into(),
                name: "English".into(),
                path: srt,
            }],
            playlist_id: Some("PL_TEST".into()),
        };
        let st = State::open(&d.path().join("state.sqlite")).unwrap();
        (d, m, st)
    }

    fn go(m: &Manifest, api: &FakeApi, st: &State) -> Result<PublishResult, PublishError> {
        publish(m, api, st, true, false, &mut |_| {})
    }

    #[test]
    fn full_publish_and_no_reupload() {
        let (_d, m, st) = setup();
        let api = FakeApi::new();
        let r = go(&m, &api, &st).unwrap();
        assert!(r.verification.ok, "{:?}", r.verification);
        assert_eq!(api.count("videos.insert"), 1);
        let r2 = go(&m, &api, &st).unwrap();
        assert_eq!(api.count("videos.insert"), 1);
        assert_eq!(r2.steps[0], "videos.insert (skipped: already uploaded)");
    }

    #[test]
    fn needs_confirmation_and_allow_public() {
        let (_d, mut m, st) = setup();
        let api = FakeApi::new();
        assert!(matches!(
            publish(&m, &api, &st, false, false, &mut |_| {}),
            Err(PublishError::Safety(_))
        ));
        m.privacy = Privacy::Public;
        assert!(matches!(go(&m, &api, &st), Err(PublishError::Safety(_))));
        assert_eq!(api.count("videos.insert"), 0);
    }

    #[test]
    fn missing_video_is_blocked() {
        let (_d, mut m, st) = setup();
        m.video = PathBuf::from("/nope/x.mp4");
        assert!(
            matches!(go(&m, &FakeApi::new(), &st), Err(PublishError::Safety(s)) if s.contains("not found"))
        );
    }

    #[test]
    fn partial_failure_resumes_without_reupload() {
        let (_d, m, st) = setup();
        let api = FakeApi::new().failing("thumbnails.set", vec![ApiError::http(400, None, "bad")]);
        assert!(go(&m, &api, &st).is_err());
        let r = go(&m, &api, &st).unwrap();
        assert_eq!(api.count("videos.insert"), 1);
        assert!(r.verification.ok);
    }

    #[test]
    fn thumbnail_403_is_a_warning() {
        let (_d, m, st) = setup();
        let api = FakeApi::new().failing(
            "thumbnails.set",
            vec![ApiError::http(403, Some("forbidden"), "no")],
        );
        let r = go(&m, &api, &st).unwrap();
        assert!(r.verification.ok && r.warnings.iter().any(|w| w.contains("verified channel")));
        assert_eq!(api.count("playlistItems.insert"), 1);
    }

    #[test]
    fn caption_conflict_is_idempotent() {
        let (_d, m, st) = setup();
        let api = FakeApi::new().failing(
            "captions.insert",
            vec![ApiError::http(409, Some("captionExists"), "dup")],
        );
        let r = go(&m, &api, &st).unwrap();
        assert!(r
            .steps
            .contains(&"captions.insert[en] (already existed)".to_string()));
    }

    #[test]
    fn saved_session_is_resumed() {
        let (_d, m, st) = setup();
        let api = FakeApi::new().failing("videos.insert", vec![ApiError::network("down")]);
        assert!(go(&m, &api, &st).is_err());
        let sha = file_sha256(&m.video).unwrap();
        let prev = st.find(&sha, "UC_TEST").unwrap().unwrap();
        assert!(prev.session.is_some() && prev.has("upload_started") && prev.video_id.is_none());
        let r = go(&m, &api, &st).unwrap();
        assert_eq!(api.sessions_used.borrow().last().unwrap(), &prev.session);
        assert!(r.steps.contains(&"videos.insert (resumed)".to_string()));
        assert!(st.find(&sha, "UC_TEST").unwrap().unwrap().session.is_none());
    }

    #[test]
    fn expired_session_finds_the_upload() {
        let (_d, m, st) = setup();
        let api = FakeApi::new();
        go(&m, &api, &st).unwrap();
        let sha = file_sha256(&m.video).unwrap();
        st.forget(&sha, "UC_TEST").unwrap();
        st.record(
            &m.id,
            &sha,
            "UC_TEST",
            None,
            Some("upload_started"),
            Some("https://upload.example/old"),
        )
        .unwrap();
        api.fail.borrow_mut().insert(
            "videos.insert".into(),
            vec![ApiError {
                kind: ErrorKind::SessionExpired,
                ..ApiError::http(404, None, "gone")
            }],
        );
        api.calls.borrow_mut().clear();
        let r = go(&m, &api, &st).unwrap();
        assert_eq!(api.count("videos.insert:resume"), 1);
        assert_eq!(api.count("videos.insert"), 0);
        assert_eq!(r.video_id, "VID1");
    }

    #[test]
    fn lost_response_without_session_is_found_by_file() {
        let (_d, m, st) = setup();
        let api = FakeApi::new();
        go(&m, &api, &st).unwrap();
        let sha = file_sha256(&m.video).unwrap();
        st.forget(&sha, "UC_TEST").unwrap();
        st.record(&m.id, &sha, "UC_TEST", None, Some("upload_started"), None)
            .unwrap();
        api.calls.borrow_mut().clear();
        let r = go(&m, &api, &st).unwrap();
        assert!(api.count("find_by_file") == 1 && api.count("videos.insert") == 0);
        assert_eq!(r.video_id, "VID1");
    }
}
