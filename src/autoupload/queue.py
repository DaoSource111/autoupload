"""Channel queue: each video in the queue folder with its sheet (.yaml) is scheduled in the next free slot and moved to the uploaded folder."""
from __future__ import annotations

import json
import os
import re
import shutil
import time
import unicodedata
from contextlib import contextmanager
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Callable, Optional

import yaml

from .channels import Channel, booked_from_youtube, next_slots
from .manifest import Manifest
from .upload import is_stop_error
from .validators import VIDEO_EXT

# Keys allowed in a video sheet: the manifest field names. Any other key is an error.
SHEET_KEYS = {"title", "description", "tags", "category_id", "default_language", "default_audio_language",
              "made_for_kids", "contains_synthetic_media", "notify_subscribers", "playlist_id", "privacy",
              "publish_at", "thumbnail", "localizations", "captions"}
THUMBNAIL_EXT = (".jpg", ".jpeg", ".png")
SETTLE_SECONDS = 120.0          # a video modified this recently may still be copying: it waits for the next run
LOCK_STALE_SECONDS = 6 * 3600   # a lock older than this is left over from a crashed run


class StopRun(RuntimeError):
    """Error that stops the whole run (authentication, quota, wrong channel)."""


@dataclass
class QueueItem:
    video: Path
    sheet: Optional[Path]
    thumbnail: Optional[Path]

    @property
    def id(self) -> str:
        s = unicodedata.normalize("NFKD", self.video.stem).encode("ascii", "ignore").decode().lower()
        s = re.sub(r"[^a-z0-9-]+", "-", s).strip("-")[:60]
        return s if len(s) >= 2 else f"video-{s or 'x'}"


@dataclass
class RunResult:
    done: list[dict] = field(default_factory=list)
    errors: list[str] = field(default_factory=list)
    plan: list[str] = field(default_factory=list)


def queue_items(channel: Channel) -> list[QueueItem]:
    if not channel.queue.exists():
        return []
    res = []
    for v in sorted(p for p in channel.queue.iterdir() if p.suffix.lower() in VIDEO_EXT):
        sheet = next((v.with_suffix(e) for e in (".yaml", ".yml", ".json") if v.with_suffix(e).exists()), None)
        thumb = next((v.with_suffix(e) for e in THUMBNAIL_EXT if v.with_suffix(e).exists()), None)
        res.append(QueueItem(v, sheet, thumb))
    return res


def read_sheet(path: Path) -> dict:
    text = path.read_text(encoding="utf-8")
    data = json.loads(text) if path.suffix == ".json" else yaml.safe_load(text)
    if not isinstance(data, dict):
        raise ValueError(f"{path.name}: the sheet must be a mapping with at least a 'title' key")
    for key in data:
        if key not in SHEET_KEYS:
            raise ValueError(f"{path.name}: unknown key '{key}' (allowed keys: {', '.join(sorted(SHEET_KEYS))})")
    return dict(data)


def build_manifest(channel: Channel, item: QueueItem, when: Optional[datetime]) -> Manifest:
    """Merges the channel's default values with the video's sheet. Without a fixed date, the video is private until slot `when`."""
    if item.sheet is None:
        raise ValueError(f"missing sheet {item.video.stem}.yaml with the title")
    f = read_sheet(item.sheet)
    if "title" not in f:
        raise ValueError(f"{item.sheet.name}: missing 'title'")
    desc = str(f.get("description", "")).rstrip()
    if channel.description_footer:
        desc = f"{desc}\n\n{channel.description_footer}".strip()
    tags = list(dict.fromkeys([*channel.base_tags, *(f.get("tags") or [])]))
    data = {
        "id": item.id, "video": item.video.name, "title": f["title"], "description": desc, "tags": tags,
        "category_id": str(f.get("category_id", channel.category)),
        "default_language": f.get("default_language", channel.language),
        "default_audio_language": f.get("default_audio_language", f.get("default_language", channel.language)),
        "made_for_kids": f.get("made_for_kids", channel.made_for_kids),
        "contains_synthetic_media": f.get("contains_synthetic_media", channel.contains_ai),
        "notify_subscribers": f.get("notify_subscribers", channel.notify_subscribers),
        "playlist_id": f.get("playlist_id", channel.playlist),
        "privacy": f.get("privacy", "private"),
    }
    if f.get("thumbnail") or item.thumbnail:
        data["thumbnail"] = f.get("thumbnail") or item.thumbnail.name
    for k in ("localizations", "captions"):
        if f.get(k):
            data[k] = f[k]
    fixed = f.get("publish_at")
    if fixed:
        data["publish_at"] = fixed if isinstance(fixed, datetime) else datetime.fromisoformat(str(fixed))
    elif data["privacy"] == "private" and when is not None:
        data["publish_at"] = when
    return Manifest.model_validate(data).resolve(channel.queue)


def is_settling(path: Path, now: Optional[float] = None) -> bool:
    """True if the file changed in the last SETTLE_SECONDS (it may still be being copied into the queue)."""
    try:
        return (now or time.time()) - path.stat().st_mtime < SETTLE_SECONDS
    except OSError:
        return True


@contextmanager
def channel_lock(channel: Channel):
    """Only one run at a time per channel (the daily task and a manual run must not upload the same queue)."""
    lock = channel.folder / "run.lock"
    for _ in range(2):
        try:
            fd = os.open(str(lock), os.O_CREAT | os.O_EXCL | os.O_WRONLY)
            break
        except FileExistsError:
            try:
                age = time.time() - lock.stat().st_mtime
            except OSError:
                continue
            if age > LOCK_STALE_SECONDS:
                lock.unlink(missing_ok=True)
                continue
            raise StopRun(f"[{channel.name}] another AutoUpload run is using this channel (started {age / 60:.0f} min ago). "
                          f"If none is running, delete {lock}") from None
    else:
        raise StopRun(f"[{channel.name}] could not take the run lock {lock}")
    try:
        stamp = datetime.now(timezone.utc).isoformat(timespec="seconds")
        os.write(fd, f"pid={os.getpid()} started={stamp}\n".encode())
        os.close(fd)
        yield
    finally:
        lock.unlink(missing_ok=True)


def process_queue(channel: Channel, client, *, execute: bool, limit: Optional[int] = None, allow_public: bool = False,
                  now: Optional[datetime] = None, notify: Callable[[str], None] = print,
                  progress: Callable[[float], None] = lambda p: None) -> RunResult:
    """Schedules the queue. With execute=False it only shows the plan (reads the channel calendar, uploads nothing)."""
    result = RunResult()
    ready = []
    for item in queue_items(channel):
        if is_settling(item.video):
            notify(f"[{channel.name}] {item.video.name} changed less than {SETTLE_SECONDS:.0f}s ago "
                   f"(still copying?); it waits for the next run")
        else:
            ready.append(item)
    pending = ready[: limit or channel.max_per_run]
    if not pending:
        notify(f"[{channel.name}] queue empty")
        return result
    if execute:
        with channel_lock(channel):
            return _process(channel, client, pending, result, allow_public=allow_public, now=now, notify=notify,
                            progress=progress, execute=True)
    return _process(channel, client, pending, result, allow_public=allow_public, now=now, notify=notify,
                    progress=progress, execute=False)


def _process(channel: Channel, client, pending: list[QueueItem], result: RunResult, *, execute: bool, allow_public: bool,
             now: Optional[datetime], notify: Callable[[str], None], progress: Callable[[float], None]) -> RunResult:
    from .publisher import SafetyError, publish, write_report
    from .state import State

    yt_channel = client.my_channel()
    expected = channel.expected_channel_id()
    if expected and yt_channel["id"] != expected:
        raise StopRun(f"[{channel.name}] the token belongs to channel '{yt_channel['title']}' ({yt_channel['id']}), "
                      f"not to {expected}. Run: autoupload auth {channel.name} login")
    notify(f"[{channel.name}] connected channel: {yt_channel['title']} ({yt_channel['id']})")

    booked = booked_from_youtube(client.uploads(), channel.tz)
    slots = next_slots(channel, len(pending), booked, now)
    state_db = State(channel.state_db)
    for item, when in zip(pending, slots):
        try:
            m = build_manifest(channel, item, when)
        except Exception as e:  # noqa: BLE001
            result.errors.append(f"{item.video.name}: {e}")
            notify(f"  ✗ {item.video.name}: {e}")
            continue
        when_text = m.publish_at.astimezone(channel.tz).strftime("%Y-%m-%d %H:%M") if m.publish_at else m.privacy.value
        line = f"{item.video.name} → '{m.title}' · {when_text}"
        result.plan.append(line)
        if not execute:
            notify(f"  · {line}")
            continue
        notify(f"  ↑ {line}")
        try:
            r = publish(m, client, state_db, confirm=True, allow_public=allow_public, on_progress=progress)
        except SafetyError as e:
            result.errors.append(f"{item.video.name}: {e}")
            notify(f"\n  ✗ blocked: {e}")
            continue
        except Exception as e:  # noqa: BLE001
            if is_stop_error(e):
                raise StopRun(f"[{channel.name}] YouTube does not accept more uploads today: {e}") from e
            result.errors.append(f"{item.video.name}: {e}")
            notify(f"\n  ✗ error (the saved state allows retrying without duplicates): {e}")
            continue
        same_file_other_item = r.get("previous_manifest_id") not in (None, m.id)
        if same_file_other_item and any(s.startswith("videos.insert (skipped: already uploaded") for s in r["steps"]):
            msg = (f"{item.video.name}: this same file was already uploaded to this channel ({r['url']}); "
                   f"it will not be uploaded again. Remove it from the queue.")
            result.errors.append(msg)
            notify(f"\n  ✗ {msg}")
            continue
        for w in r.get("warnings", []):
            notify(f"\n  ! {w}")
        dest = channel.uploaded / f"{datetime.now(timezone.utc).astimezone(channel.tz):%Y-%m-%d}_{item.id}"
        dest.mkdir(parents=True, exist_ok=True)
        write_report(r, dest / "report.json")
        for p in (item.video, item.sheet, item.thumbnail):
            if p and p.exists():
                shutil.move(str(p), str(dest / p.name))
        result.done.append({"video": item.video.name, "url": r["url"], "date": when_text,
                            "verified": r["verification"]["ok"]})
        notify(f"\n  ✓ {r['url']} ({'verified' if r['verification']['ok'] else 'check: ' + ', '.join(r['verification']['problems'])})")
    return result


def calendar_rows(channel: Channel, client, days: int = 30, now: Optional[datetime] = None) -> list[tuple[datetime, str, str]]:
    """Channel uploads between today and `days` days from now, plus those of the last 3 days."""
    now = now or datetime.now(timezone.utc)
    rows = []
    for v in client.uploads():
        for when in booked_from_youtube([v], channel.tz):
            if -3 <= (when - now).total_seconds() / 86400 <= days:
                rows.append((when, v["status"].get("privacyStatus", "?"), v["snippet"].get("title", "")))
    return sorted(rows)
