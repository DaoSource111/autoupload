from datetime import datetime, timedelta, timezone

import pytest
from pydantic import ValidationError

from autoupload.manifest import Manifest, load_manifest
from autoupload.metadata import insert_body, merge_for_update
from autoupload import validators
from autoupload.validators import validate, validate_caption_text


def test_default_privacy_is_private_and_explicit(manifest):
    body, part = insert_body(manifest)
    assert body["status"]["privacyStatus"] == "private"
    assert part == "snippet,status,localizations"
    assert body["snippet"]["defaultLanguage"] == "en"
    assert body["localizations"]["de"]["title"] == "Was waere, wenn Dunkelheit Gewicht haette?"


def test_paths_resolved_relative_to_manifest(manifest, workdir):
    assert manifest.video == (workdir / "video.mp4").resolve()


@pytest.mark.parametrize("over,msg", [
    ({"title": "x" * 101}, "maximum 100"),
    ({"title": "   "}, "empty"),
    ({"default_language": "english"}, "BCP-47"),
    ({"localizations": {"en": {"title": "dup"}}}, "default language"),
    ({"tags": ["a" * 300, "b" * 300]}, "maximum 500"),
    ({"description": "<script>"}, "< or >"),
    ({"privacy": "public", "publish_at": "2099-01-01T10:00:00Z"}, "privacy: private"),
    ({"publish_at": "2001-01-01T10:00:00Z"}, "future"),
    ({"captions": [{"language": "en", "name": "A", "path": "en.srt"}, {"language": "en", "name": "A", "path": "en.srt"}]}, "captionExists"),
])
def test_invalid_metadata_rejected(manifest_file, over, msg):
    with pytest.raises(ValidationError) as e:
        load_manifest(manifest_file(**over))
    assert msg in str(e.value)


def test_scheduled_publish_body(manifest_file):
    when = (datetime.now(timezone.utc) + timedelta(days=2)).replace(microsecond=0)
    m = load_manifest(manifest_file(publish_at=when.isoformat()))
    body, _ = insert_body(m)
    assert body["status"]["publishAt"].endswith("Z")


def test_missing_files_detected(manifest_file, workdir):
    (workdir / "en.srt").unlink()
    (workdir / "thumb.png").write_bytes(b"not an image")
    rep = validate(load_manifest(manifest_file()))
    assert not rep.ok
    assert any("caption file not found [en]" in e for e in rep.errors)
    assert any("JPEG/PNG" in e for e in rep.errors)


def test_video_missing(manifest_file, workdir):
    (workdir / "video.mp4").unlink()
    rep = validate(load_manifest(manifest_file()))
    assert any("video file not found" in e for e in rep.errors)


def test_caption_timing_errors():
    bad = "1\n00:00:05,000 --> 00:00:04,000\nA\n\n2\n00:00:03,000 --> 00:00:09,000\nB\n"
    errs = validate_caption_text(bad, ".srt", video_duration=6.0)
    assert any("not after start" in e for e in errs)
    assert any("overlaps" in e for e in errs)
    assert any("after the video" in e for e in errs)
    assert "VTT is missing the WEBVTT header" in validate_caption_text("00:00:00.000 --> 00:00:01.000\nx", ".vtt")


def test_dubbed_audio_duration_mismatch(manifest, monkeypatch):
    durations = {"video.mp4": 120.0, "audio-en.wav": 110.0}
    monkeypatch.setattr(validators, "find_ffprobe", lambda x=None: "ffprobe")
    monkeypatch.setattr(validators, "probe_duration", lambda p, fp: durations[p.name])
    rep = validate(manifest)
    assert any("difference 10.00s" in e for e in rep.errors)
    durations["audio-en.wav"] = 120.8
    assert validate(manifest).ok


def test_update_merge_preserves_existing_fields():
    current = {"id": "V1", "snippet": {"title": "T", "categoryId": "24", "description": "D", "tags": ["a"],
                                       "publishedAt": "x", "channelId": "UC"},
               "status": {"privacyStatus": "private", "uploadStatus": "processed"}}
    body = merge_for_update(current, {"snippet": {"description": "New"}}, ["snippet"])
    assert body["snippet"] == {"title": "T", "categoryId": "24", "description": "New", "tags": ["a"]}
    with pytest.raises(ValueError):
        merge_for_update({"id": "V", "snippet": {"description": "x"}}, {}, ["snippet"])
