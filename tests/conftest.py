"""Test doubles: a fake YouTube that records calls and can fail on demand. Nothing touches the network."""
from __future__ import annotations

from pathlib import Path

import pytest
import yaml

from subidor.manifest import load_manifest

PNG = b"\x89PNG\r\n\x1a\n" + b"\x00" * 64
SRT = "1\n00:00:00,000 --> 00:00:02,000\nHola\n\n2\n00:00:02,500 --> 00:00:04,000\nAdiós\n"


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
        return {"id": "UC_TEST", "title": "Canal de prueba"}

    def insert_video(self, body, part, path, notify, on_progress=lambda p: None):
        self.calls.append("videos.insert")
        self._maybe_fail("videos.insert")
        self.n += 1
        vid = f"VID{self.n}"
        self.videos[vid] = {"id": vid, "snippet": dict(body["snippet"]), "status": {**body["status"], "uploadStatus": "uploaded"},
                            "localizations": body.get("localizations", {})}
        self.last_body, self.last_part, self.last_notify = body, part, notify
        on_progress(1.0)
        return {"id": vid}

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
            "id": "ep-prueba", "video": "video.mp4", "thumbnail": "thumb.png",
            "default_language": "es", "default_audio_language": "es",
            "title": "¿Y si la oscuridad tuviera peso?", "description": "Prueba", "tags": ["what if"],
            "localizations": {"en": {"title": "What if darkness had weight?", "description": "Test"}},
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
