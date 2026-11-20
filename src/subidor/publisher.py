"""Staged orchestration: plan (dry run) → upload → thumbnail → captions → playlist → verification → report.

Each stage is recorded in SQLite; if the process is interrupted, `publish` resumes where it stopped without uploading again.
"""
from __future__ import annotations

import json
import logging
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Optional

from .manifest import Manifest, Privacy
from .metadata import insert_body
from .state import State, file_sha256
from .upload import http_status
from .validators import Report, validate

log = logging.getLogger(__name__)

QUOTA = {"videos.insert": 1, "thumbnails.set": 50, "captions.insert": 400, "playlistItems.insert": 50,
         "videos.list": 1, "channels.list": 1}
UPLOAD_BUCKET = {"videos.insert"}  # "Video Uploads" bucket (100 calls/day), separate from the 10 000 units


class SafetyError(RuntimeError):
    pass


@dataclass
class Plan:
    steps: list[tuple[str, str]] = field(default_factory=list)  # (method, description)
    studio: list[str] = field(default_factory=list)
    report: Optional[Report] = None

    @property
    def quota_units(self) -> int:
        return sum(QUOTA[m] for m, _ in self.steps if m not in UPLOAD_BUCKET)

    @property
    def upload_calls(self) -> int:
        return sum(1 for m, _ in self.steps if m in UPLOAD_BUCKET)

    def render(self) -> str:
        lines = ["PLAN (nothing has been changed on YouTube)"]
        for m, d in self.steps:
            lines.append(f"  · {m:<22} {d}")
        lines.append(f"Estimated quota: {self.quota_units} units + {self.upload_calls} upload(s) from the upload bucket")
        if self.studio:
            lines.append("Pending in YouTube Studio (no public API):")
            lines += [f"  · {s}" for s in self.studio]
        if self.report:
            lines += [f"ERROR: {e}" for e in self.report.errors]
            lines += [f"warning: {w}" for w in self.report.warnings]
        return "\n".join(lines)


def make_plan(m: Manifest, ffprobe: Optional[str] = None, done: Optional[list[str]] = None) -> Plan:
    done = done or []
    p = Plan(report=validate(m, ffprobe))
    p.steps.append(("channels.list", "identify the connected channel"))
    if "uploaded" not in done:
        langs = ", ".join(m.localizations) or "none"
        when = f", scheduled {m.publish_at.isoformat()}" if m.publish_at else ""
        p.steps.append(("videos.insert", f"upload {m.video.name} as {m.privacy.value}{when}; localizations: {langs}"))
    if m.thumbnail and "thumbnail" not in done:
        p.steps.append(("thumbnails.set", f"thumbnail {m.thumbnail.name}"))
    if "captions" not in done:
        for c in m.captions:
            p.steps.append(("captions.insert", f"captions [{c.language}] {c.path.name}"))
    if m.playlist_id and "playlist" not in done:
        p.steps.append(("playlistItems.insert", f"add to playlist {m.playlist_id}"))
    p.steps.append(("videos.list", "verify status, privacy and localizations"))
    p.studio += [f"dubbing [{a.language}] {a.path.name} ({a.status.value})" for a in m.audio_tracks]
    p.studio += [f"thumbnail [{t.language}] {t.path.name} ({t.status.value})" for t in m.localized_thumbnails]
    return p


def publish(m: Manifest, client, state: State, *, confirm: bool, allow_public: bool = False,
            force_new: bool = False, ffprobe: Optional[str] = None, on_progress=lambda p: None) -> dict:
    """Runs the plan against YouTube. Requires confirm=True; `public` also requires allow_public=True."""
    rep = validate(m, ffprobe)
    if not rep.ok:
        raise SafetyError("the manifest did not pass validation:\n- " + "\n- ".join(rep.errors))
    if not confirm:
        raise SafetyError("explicit confirmation (--yes) is required to modify real content")
    if m.privacy == Privacy.public and not allow_public:
        raise SafetyError("the manifest asks for `public`: add --allow-public if you really want to publish it")

    channel = client.my_channel()
    sha = file_sha256(m.video)
    prev = state.find(sha, channel["id"])
    if prev and prev["video_id"] and force_new:
        state.forget(sha, channel["id"])
        prev = None
    stages = prev["stages"] if prev else []
    video_id = prev["video_id"] if prev else None
    result = {"manifest_id": m.id, "channel": channel, "video_id": video_id, "resumed": bool(prev), "steps": [],
              "studio_pending": [a.language for a in m.audio_tracks]}

    state.record(m.id, sha, channel["id"], stage="validated")
    if "uploaded" not in stages:
        body, part = insert_body(m)
        resp = client.insert_video(body, part, m.video, m.notify_subscribers, on_progress)
        video_id = resp["id"]
        state.record(m.id, sha, channel["id"], video_id=video_id, stage="uploaded")
        state.event(m.id, "uploaded", video_id)
        result["steps"].append("videos.insert")
    else:
        result["steps"].append("videos.insert (skipped: already uploaded)")

    if m.thumbnail and "thumbnail" not in stages:
        client.set_thumbnail(video_id, m.thumbnail)
        state.record(m.id, sha, channel["id"], stage="thumbnail")
        result["steps"].append("thumbnails.set")

    if m.captions and "captions" not in stages:
        for c in m.captions:
            try:
                client.insert_caption(video_id, c.language, c.name, c.path)
                result["steps"].append(f"captions.insert[{c.language}]")
            except Exception as exc:  # noqa: BLE001
                if http_status(exc) == 409:  # captionExists: idempotent
                    result["steps"].append(f"captions.insert[{c.language}] (already existed)")
                else:
                    raise
        state.record(m.id, sha, channel["id"], stage="captions")

    if m.playlist_id and "playlist" not in stages:
        client.add_to_playlist(m.playlist_id, video_id)
        state.record(m.id, sha, channel["id"], stage="playlist")
        result["steps"].append("playlistItems.insert")

    result["video_id"] = video_id
    result["verification"] = verify(m, client, video_id)
    if result["verification"]["ok"]:
        state.record(m.id, sha, channel["id"], stage="verified")
    result["url"] = f"https://www.youtube.com/watch?v={video_id}"
    result["studio_url"] = f"https://studio.youtube.com/video/{video_id}/edit"
    return result


def verify(m: Manifest, client, video_id: str) -> dict:
    v = client.get_video(video_id)
    if v is None:
        return {"ok": False, "problems": ["videos.list did not return the video"]}
    problems = []
    st, sn = v.get("status", {}), v.get("snippet", {})
    if st.get("privacyStatus") != m.privacy.value:
        problems.append(f"privacy {st.get('privacyStatus')} ≠ {m.privacy.value}")
    if sn.get("title") != m.title:
        problems.append("title does not match")
    missing = set(m.localizations) - set(v.get("localizations", {}))
    if missing:
        problems.append(f"missing localizations: {sorted(missing)}")
    return {"ok": not problems, "problems": problems,
            "upload_status": st.get("uploadStatus"),
            "processing": v.get("processingDetails", {}).get("processingStatus")}


def write_report(result: dict, out: Path) -> Path:
    out.parent.mkdir(parents=True, exist_ok=True)
    result = {**result, "generated": datetime.now(timezone.utc).isoformat(timespec="seconds")}
    out.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8")
    return out
