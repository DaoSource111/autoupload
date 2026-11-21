import json

import pytest

from autoupload import channels, cli
from autoupload import auth
from autoupload.manifest import load_manifest
from autoupload.publisher import SafetyError, make_plan, publish
from autoupload.state import State, file_sha256
from autoupload.studio_package import build, mark_done
from autoupload.upload import UploadError, run_resumable
from conftest import FakeClient, FakeHttpError


@pytest.fixture
def state(tmp_path):
    s = State(tmp_path / "state.sqlite")
    yield s
    s.close()


def test_dry_run_plan_makes_no_remote_calls(manifest):
    plan = make_plan(manifest)
    methods = [m for m, _ in plan.steps]
    assert methods == ["channels.list", "videos.insert", "thumbnails.set", "captions.insert", "playlistItems.insert", "videos.list"]
    assert plan.quota_units == 1 + 50 + 400 + 50 + 1 and plan.upload_calls == 1
    assert any("dubbing [en]" in s for s in plan.studio)
    assert "nothing has been changed" in plan.render()


def test_publish_requires_confirmation(manifest, state):
    c = FakeClient()
    with pytest.raises(SafetyError, match="confirmation"):
        publish(manifest, c, state, confirm=False)
    assert c.calls == []


def test_public_requires_allow_public(manifest_file, state):
    m = load_manifest(manifest_file(privacy="public"))
    with pytest.raises(SafetyError, match="allow-public"):
        publish(m, FakeClient(), state, confirm=True)


def test_invalid_manifest_blocks_before_any_call(manifest_file, workdir, state):
    (workdir / "video.mp4").unlink()
    c = FakeClient()
    with pytest.raises(SafetyError):
        publish(load_manifest(manifest_file()), c, state, confirm=True)
    assert c.calls == []


def test_full_publish_and_verification(manifest, state):
    c = FakeClient()
    res = publish(manifest, c, state, confirm=True)
    assert c.calls == ["channels.list", "videos.insert", "thumbnails.set", "captions.insert:en", "playlistItems.insert", "videos.list"]
    assert c.last_notify is False
    assert res["verification"]["ok"] and res["video_id"] == "VID1"
    assert res["url"].endswith("VID1") and res["studio_pending"] == ["en"]


def test_idempotent_second_run_does_not_reupload(manifest, state):
    c = FakeClient()
    publish(manifest, c, state, confirm=True)
    c.calls.clear()
    res = publish(manifest, c, state, confirm=True)
    assert "videos.insert" not in c.calls and res["resumed"] and res["video_id"] == "VID1"


def test_force_new_uploads_again(manifest, state):
    c = FakeClient()
    publish(manifest, c, state, confirm=True)
    res = publish(manifest, c, state, confirm=True, force_new=True)
    assert res["video_id"] == "VID2"


def test_partial_failure_resumes_without_reupload(manifest, state):
    c = FakeClient(fail={"thumbnails.set": [FakeHttpError(400)]})
    with pytest.raises(FakeHttpError):
        publish(manifest, c, state, confirm=True)
    assert c.calls.count("videos.insert") == 1
    c.calls.clear()
    res = publish(manifest, c, state, confirm=True)
    assert "videos.insert" not in c.calls and "thumbnails.set" in c.calls and res["verification"]["ok"]


def test_thumbnail_rejected_for_unverified_channel_does_not_fail_the_video(manifest, state):
    c = FakeClient(fail={"thumbnails.set": [FakeHttpError(403)]})
    res = publish(manifest, c, state, confirm=True)
    assert res["verification"]["ok"] and "playlistItems.insert" in c.calls
    assert any("verified channel" in w for w in res["warnings"])


def test_saved_session_is_resumed(manifest, state):
    c = FakeClient(fail={"videos.insert": [ConnectionError("network down")]})
    with pytest.raises(ConnectionError):
        publish(manifest, c, state, confirm=True)
    prev = state.find(file_sha256(manifest.video), "UC_TEST")
    assert prev["session"] and "upload_started" in prev["stages"] and not prev["video_id"]
    res = publish(manifest, c, state, confirm=True)
    assert c.sessions_used[-1] == prev["session"] and "videos.insert (resumed)" in res["steps"]
    assert state.find(file_sha256(manifest.video), "UC_TEST")["session"] is None


def test_expired_session_finds_the_upload_instead_of_duplicating(manifest, state):
    from autoupload.upload import SessionExpired
    c = FakeClient()
    publish(manifest, c, state, confirm=True)            # the video is on the channel...
    sha = file_sha256(manifest.video)
    state.forget(sha, "UC_TEST")                         # ...but pretend the response was lost mid-upload
    state.record(manifest.id, sha, "UC_TEST", stage="upload_started", session="https://upload.example/old")
    c.fail = {"videos.insert": [SessionExpired("gone")]}
    c.calls.clear()
    res = publish(manifest, c, state, confirm=True)
    assert c.calls.count("videos.insert:resume") == 1 and "videos.insert" not in c.calls
    assert res["video_id"] == "VID1" and "found on the channel" in res["steps"][0]


def test_lost_response_without_session_is_found_by_file(manifest, state):
    c = FakeClient()
    publish(manifest, c, state, confirm=True)
    sha = file_sha256(manifest.video)
    state.forget(sha, "UC_TEST")
    state.record(manifest.id, sha, "UC_TEST", stage="upload_started")
    c.calls.clear()
    res = publish(manifest, c, state, confirm=True)
    assert "find_by_file" in c.calls and not any(x.startswith("videos.insert") for x in c.calls)
    assert res["video_id"] == "VID1"


def test_caption_conflict_is_idempotent(manifest, state):
    c = FakeClient(fail={"captions.insert": [FakeHttpError(409)]})
    res = publish(manifest, c, state, confirm=True)
    assert "captions.insert[en] (already existed)" in res["steps"]


def test_insufficient_permissions_propagates(manifest, state):
    c = FakeClient(fail={"videos.insert": [FakeHttpError(403)]})
    with pytest.raises(FakeHttpError):
        publish(manifest, c, state, confirm=True)


def test_verification_detects_mismatch(manifest, state):
    c = FakeClient()
    orig = c.insert_video

    def lossy(*a, **k):
        r = orig(*a, **k)
        c.videos[r["id"]]["localizations"] = {}
        c.videos[r["id"]]["status"]["privacyStatus"] = "public"
        return r
    c.insert_video = lossy
    res = publish(manifest, c, state, confirm=True)
    assert not res["verification"]["ok"]
    assert any("privacy" in p for p in res["verification"]["problems"])


# --- resumable upload ---------------------------------------------------------------------------
class _Status:
    def __init__(self, p): self.p = p
    def progress(self): return self.p


class FakeRequest:
    def __init__(self, script):
        self.script = list(script)

    def next_chunk(self):
        item = self.script.pop(0)
        if isinstance(item, Exception):
            raise item
        return item


def test_resumable_retries_recoverable_errors():
    req = FakeRequest([FakeHttpError(503), (_Status(0.5), None), ConnectionError("reset"), (None, {"id": "V9"})])
    waits, prog = [], []
    resp = run_resumable(req, prog.append, sleep=waits.append)
    assert resp["id"] == "V9" and len(waits) == 2 and prog[-1] == 1.0


def test_resumable_does_not_retry_client_errors():
    with pytest.raises(UploadError, match="non-recoverable"):
        run_resumable(FakeRequest([FakeHttpError(400)]), sleep=lambda s: None)


def test_resumable_gives_up_after_max_retries():
    with pytest.raises(UploadError, match="too many"):
        run_resumable(FakeRequest([FakeHttpError(500)] * 5), sleep=lambda s: None, max_retries=3)


def test_incomplete_response_is_an_error():
    with pytest.raises(UploadError, match="without id"):
        run_resumable(FakeRequest([(None, {"kind": "youtube#video"})]), sleep=lambda s: None)


# --- Studio package and CLI ----------------------------------------------------------------------
def test_studio_package_tracks_status_separately(manifest, tmp_path):
    out = build(manifest, tmp_path / "pkg", "VID1")
    data = json.loads((out / "checklist.json").read_text(encoding="utf-8"))
    assert data["audio_tracks"]["en"]["status"] == "prepared"
    assert (out / "audio-en.wav").exists() and "Languages" in (out / "STUDIO-STEPS.md").read_text(encoding="utf-8")
    assert mark_done(out, "en")["audio_tracks"]["en"]["status"] == "uploaded_in_studio"


def test_cli_validate_and_plan(manifest_file, capsys):
    p = manifest_file()
    assert cli.main(["validate", str(p)]) == cli.OK
    assert cli.main(["plan", str(p)]) == cli.OK
    assert "PLAN" in capsys.readouterr().out


def test_cli_publish_without_yes_uploads_nothing(manifest_file, capsys, monkeypatch, tmp_path):
    monkeypatch.setenv("AUTOUPLOAD_HOME", str(tmp_path / "home"))
    channels.create_channel("test")
    monkeypatch.setattr(auth, "load", lambda *a, **k: pytest.fail("must not authenticate without --yes"))
    assert cli.main(["publish", str(manifest_file()), "--channel", "test"]) == cli.OK
    assert "Nothing has been uploaded" in capsys.readouterr().out


def test_cli_expired_credentials(manifest_file, monkeypatch, tmp_path):
    monkeypatch.setenv("AUTOUPLOAD_HOME", str(tmp_path / "home"))
    channels.create_channel("test")

    def expired(*a, **k):
        raise auth.AuthError("permission expired")
    monkeypatch.setattr(auth, "load", expired)
    assert cli.main(["publish", str(manifest_file()), "--channel", "test", "--yes"]) == cli.AUTH
    assert cli.main(["schedule", "test", "--yes"]) == cli.OK  # empty queue: does not even authenticate
    (tmp_path / "home" / "channels" / "test" / "queue" / "a.mp4").write_bytes(b"x")
    assert cli.main(["schedule", "test", "--yes"]) == cli.AUTH


def test_cli_bad_manifest(tmp_path):
    bad = tmp_path / "m.yaml"
    bad.write_text("id: X\n", encoding="utf-8")
    assert cli.main(["validate", str(bad)]) == cli.INVALID
