"""Test doubles: a fake YouTube that records calls and can fail on demand. Nothing touches the network."""
from __future__ import annotations

from pathlib import Path

import pytest
import yaml

from autoupload.manifest import load_manifest

PNG = b"\x89PNG\r\n\x1a\n" + b"\x00" * 64
SRT = "1\n00:00:00,000 --> 00:00:02,000\nHello\n\n2\n00:00:02,500 --> 00:00:04,000\nGoodbye\n"


class FakeHttpError(Exception):
    def __init__(self, status: int):
        super().__init__(f"HTTP {status}")
        self.resp = type("R", (), {"status": status})()


class FakeClient:
    def __init__(self, fail: dict | None = None):
        self.calls: list[str] = []
        self.fail = fail or {}      # method -> list of exceptions to raise, in order
        self.videos: dict[str, dict] = {}
        self.n = 0

    def _maybe_fail(self, name):
        q = self.fail.get(name)
        if q:
            raise q.pop(0)

    def my_channel(self):
        self.calls.append("channels.list")
        self._maybe_fail("channels.list")
        return {"id": "UC_TEST", "title": "Test channel"}

    def insert_video(self, body, part, path, notify, on_progress=lambda p: None, *, session_uri=None,
                     on_session=lambda uri: None):
        self.calls.append("videos.insert" + (":resume" if session_uri else ""))
        self.sessions_used = getattr(self, "sessions_used", []) + [session_uri]
        on_session(f"https://upload.example/session/{self.n + 1}")
        self._maybe_fail("videos.insert")
        self.n += 1
        vid = f"VID{self.n}"
        self.videos[vid] = {"id": vid, "snippet": dict(body["snippet"]), "status": {**body["status"], "uploadStatus": "uploaded"},
                            "localizations": body.get("localizations", {}),
                            "fileDetails": {"fileName": Path(path).name, "fileSize": Path(path).stat().st_size}}
        self.last_body, self.last_part, self.last_notify = body, part, notify
        on_progress(1.0)
        return {"id": vid}

    def find_by_file(self, file_name, file_size, recent=50):
        self.calls.append("find_by_file")
        for vid, v in self.videos.items():
            fd = v.get("fileDetails", {})
            if fd.get("fileName") == file_name and fd.get("fileSize") == file_size:
                return vid
        return None

    def set_thumbnail(self, video_id, path):
        self.calls.append("thumbnails.set")
        self._maybe_fail("thumbnails.set")
        return {}

    def insert_caption(self, video_id, language, name, path):
        self.calls.append(f"captions.insert:{language}")
        self._maybe_fail("captions.insert")
        return {}

    def add_to_playlist(self, playlist_id, video_id):
        self.calls.append("playlistItems.insert")
        self._maybe_fail("playlistItems.insert")
        return {}

    def get_video(self, video_id, parts=""):
        self.calls.append("videos.list")
        return self.videos.get(video_id)


@pytest.fixture(autouse=True)
def _no_settle_wait(monkeypatch):
    """Test videos are written a moment before use; the 'still copying' wait is tested on its own."""
    monkeypatch.setattr("autoupload.queue.SETTLE_SECONDS", 0.0)


@pytest.fixture
def workdir(tmp_path: Path) -> Path:
    (tmp_path / "video.mp4").write_bytes(b"\x00\x00\x00\x18ftypmp42" + b"\x00" * 2048)
    (tmp_path / "thumb.png").write_bytes(PNG)
    (tmp_path / "en.srt").write_text(SRT, encoding="utf-8")
    (tmp_path / "audio-en.wav").write_bytes(b"RIFF" + b"\x00" * 64)
    return tmp_path


@pytest.fixture
def manifest_file(workdir: Path):
    def make(**over) -> Path:
        data = {
            "id": "ep-test", "video": "video.mp4", "thumbnail": "thumb.png",
            "default_language": "en", "default_audio_language": "en",
            "title": "What if darkness had weight?", "description": "Test", "tags": ["what if"],
            "localizations": {"de": {"title": "Was waere, wenn Dunkelheit Gewicht haette?", "description": "Test"}},
            "captions": [{"language": "en", "name": "English", "path": "en.srt"}],
            "audio_tracks": [{"language": "en", "path": "audio-en.wav"}],
            "playlist_id": "PL_TEST",
        }
        data.update(over)
        p = workdir / "manifest.yaml"
        p.write_text(yaml.safe_dump(data, allow_unicode=True), encoding="utf-8")
        return p
    return make


@pytest.fixture
def manifest(manifest_file):
    return load_manifest(manifest_file())
