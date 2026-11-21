"""Channels: per-channel configuration (channel.yaml) and calculation of the next free publishing slots."""
from __future__ import annotations

import os
import re
import shutil
import sys
from datetime import date, datetime, time, timedelta, timezone, tzinfo
from zoneinfo import ZoneInfo, ZoneInfoNotFoundError
from pathlib import Path
from typing import Iterable, Iterator, Optional

import yaml
from pydantic import BaseModel, ConfigDict, Field, ValidationError, field_validator

NAME_RE = re.compile(r"^[a-z0-9][a-z0-9_-]{0,40}$")
TEMPLATE = """\
# Configuration for channel '{name}'. See README.md, section "channel.yaml".
profile: {name}                  # OAuth token name; created with: autoupload auth {name} login
client_secret: ../../config/client_secret.json   # Google Cloud desktop OAuth credential (can be the same for every channel)
timezone: "+00:00"               # zone of the hours below: a name such as Europe/Madrid or America/Lima (follows daylight saving), or a fixed offset such as +01:00
hours: ["08:00"]                 # publishing hours for each day (one or several)
days_between: 1                  # 1 = every day · 2 = every other day · 7 = once a week
min_lead_hours: 3                # never schedule less than this many hours from now
max_per_run: 5                   # maximum videos per run (the API limits daily uploads)
language: en                     # language of the title, description and audio
category: "27"                   # 24 Entertainment · 27 Education · 28 Science & Technology · 22 People & Blogs
made_for_kids: false             # "made for kids" declaration (required by law)
contains_ai: null                # true if the video has realistic synthetic voice or imagery; null = do not declare
notify_subscribers: false        # notify subscribers when publishing
playlist: null                   # ID of a playlist (PL...) where each video is added
base_tags: []                    # tags added to every video
description_footer: ""           # text appended to the end of every description
"""

# Keys set by autoupload itself; they can never be written in channel.yaml.
RESERVED_KEYS = ("name", "folder")


class Channel(BaseModel):
    """One channel as read from channel.yaml. Unknown keys are rejected, so old or misspelled keys fail loudly."""
    model_config = ConfigDict(extra="forbid")

    name: str
    folder: Path
    profile: str
    client_secret: Path
    timezone: str = "+00:00"
    hours: list[str] = ["08:00"]
    days_between: int = Field(1, ge=1, le=30)
    min_lead_hours: float = Field(3, ge=0)
    max_per_run: int = Field(5, ge=1, le=50)
    language: str = "en"
    category: str = "27"
    made_for_kids: bool = False
    contains_ai: Optional[bool] = None
    notify_subscribers: bool = False
    playlist: Optional[str] = None
    base_tags: list[str] = []
    description_footer: str = ""

    @field_validator("timezone")
    @classmethod
    def _timezone(cls, v: str) -> str:
        if re.match(r"^[+-]\d{2}:\d{2}$", v):
            return v
        try:
            ZoneInfo(v)
        except (ZoneInfoNotFoundError, ValueError) as e:
            raise ValueError(f"timezone {v!r} is neither a zone name like Europe/Madrid nor an offset like +01:00") from e
        return v

    @field_validator("hours")
    @classmethod
    def _hours(cls, v: list[str]) -> list[str]:
        if not v:
            raise ValueError("hours cannot be empty")
        for h in v:
            if not re.match(r"^\d{2}:\d{2}$", h):
                raise ValueError(f"invalid hour: {h!r} (use HH:MM)")
        return sorted(v)

    @property
    def tz(self) -> tzinfo:
        # `timezone` above is the field (a zone name or an offset string); the call below is the datetime class.
        if self.timezone[0] not in "+-":
            return ZoneInfo(self.timezone)
        sign = -1 if self.timezone[0] == "-" else 1
        hh, mm = self.timezone[1:].split(":")
        return timezone(sign * timedelta(hours=int(hh), minutes=int(mm)))

    @property
    def queue(self) -> Path:
        return self.folder / "queue"

    @property
    def uploaded(self) -> Path:
        return self.folder / "uploaded"

    @property
    def state_db(self) -> Path:
        return self.folder / "state.sqlite"

    @property
    def channel_id_file(self) -> Path:
        return self.folder / "channel_id.txt"

    def expected_channel_id(self) -> Optional[str]:
        f = self.channel_id_file
        return f.read_text(encoding="utf-8").strip() if f.exists() else None


def _package_parent() -> Path:
    """Two levels above the package: the repository root in a source checkout, `.venv/Lib` when pip-installed."""
    return Path(__file__).resolve().parents[2]


def _is_checkout(folder: Path) -> bool:
    return (folder / "pyproject.toml").exists() and (folder / "src" / "autoupload").is_dir()


def default_home() -> Path:
    """Data folder of an installed copy: Documents\\AutoUpload on Windows, ~/AutoUpload elsewhere."""
    if sys.platform == "win32":
        return Path.home() / "Documents" / "AutoUpload"
    return Path.home() / "AutoUpload"


def root() -> Path:
    """Folder that holds channels/ and config/.

    AUTOUPLOAD_HOME if set; else the repository when running from a source checkout (`pip install -e .`);
    else default_home(). Never the folder of the installed package, which a new venv would wipe.
    """
    env = os.environ.get("AUTOUPLOAD_HOME")
    if env:
        return Path(env)
    parent = _package_parent()
    return parent if _is_checkout(parent) else default_home()


def legacy_root() -> Optional[Path]:
    """Where versions up to 1.1.0 kept channels/ and config/ when pip-installed (inside the venv), if data is there."""
    parent = _package_parent()
    if _is_checkout(parent) or parent == root():
        return None
    return parent if (parent / "channels").is_dir() or (parent / "config").is_dir() else None


def migrate_legacy(execute: bool, src: Optional[Path] = None, dest: Optional[Path] = None) -> list[str]:
    """Moves channels and OAuth client files left inside the venv by 1.1.0 to the data folder.

    Never overwrites: anything that already exists at the destination is reported and left where it is.
    With execute=False it only lists what it would do.
    """
    src = src or legacy_root()
    dest = dest or root()
    if src is None:
        return []
    actions: list[str] = []
    moves: list[tuple[Path, Path]] = []
    old_channels = src / "channels"
    if old_channels.is_dir():
        for ch in sorted(p for p in old_channels.iterdir() if p.is_dir()):
            moves.append((ch, dest / "channels" / ch.name))
    old_config = src / "config"
    if old_config.is_dir():
        for f in sorted(old_config.glob("client_secret*.json")):
            moves.append((f, dest / "config" / f.name))
    for a, b in moves:
        if b.exists():
            actions.append(f"skip {a} (already exists: {b})")
            continue
        actions.append(f"move {a} → {b}")
        if execute:
            b.parent.mkdir(parents=True, exist_ok=True)
            shutil.move(str(a), str(b))
    return actions


def channels_dir(base: Optional[Path] = None) -> Path:
    return (base or root()) / "channels"


def list_channels(base: Optional[Path] = None) -> list[str]:
    c = channels_dir(base)
    return sorted(p.name for p in c.iterdir() if (p / "channel.yaml").exists()) if c.exists() else []


def load_channel(name: str, base: Optional[Path] = None) -> Channel:
    folder = channels_dir(base) / name
    file = folder / "channel.yaml"
    if not file.exists():
        raise FileNotFoundError(f"channel '{name}' does not exist ({file}). Create it with: autoupload channel new {name}")
    data = yaml.safe_load(file.read_text(encoding="utf-8")) or {}
    if not isinstance(data, dict):
        raise ValueError(f"{file}: channel.yaml must contain a mapping of settings")
    clash = [k for k in RESERVED_KEYS if k in data]
    if clash:
        raise ValueError(f"{file}: '{clash[0]}' is set by autoupload and cannot be written in channel.yaml")
    data.setdefault("profile", name)
    secret = Path(data.get("client_secret", "../../config/client_secret.json"))
    data["client_secret"] = secret if secret.is_absolute() else (folder / secret).resolve()
    try:
        return Channel.model_validate({**data, "name": name, "folder": folder})
    except ValidationError as e:
        raise ValueError(f"{file}: {e}") from e


def create_channel(name: str, base: Optional[Path] = None) -> Path:
    if not NAME_RE.match(name):
        raise ValueError("channel name may only contain lowercase letters, digits, - and _")
    folder = channels_dir(base) / name
    if (folder / "channel.yaml").exists():
        raise FileExistsError(f"channel '{name}' already exists")
    for sub in ("queue", "uploaded"):
        (folder / sub).mkdir(parents=True, exist_ok=True)
    (folder / "channel.yaml").write_text(TEMPLATE.format(name=name), encoding="utf-8")
    return folder


# --- dates ------------------------------------------------------------------------------------------

def booked_from_youtube(videos: Iterable[dict], tz: tzinfo) -> list[datetime]:
    """Dates already used on the channel: scheduled (publishAt) and published (publishedAt), in the channel's time zone."""
    res = []
    for v in videos:
        st, sn = v.get("status", {}), v.get("snippet", {})
        when = st.get("publishAt") if st.get("privacyStatus") == "private" else None
        if not when and st.get("privacyStatus") in ("public", "unlisted"):
            when = sn.get("publishedAt")
        if when:
            res.append(datetime.fromisoformat(when.replace("Z", "+00:00")).astimezone(tz))
    return res


def _slots(channel: Channel, since: datetime) -> Iterator[datetime]:
    d = since.astimezone(channel.tz).date()
    while True:
        for h in channel.hours:
            hh, mm = map(int, h.split(":"))
            yield datetime.combine(d, time(hh, mm), tzinfo=channel.tz)
        d += timedelta(days=1)


def next_slots(channel: Channel, n: int, booked: Iterable[datetime], now: Optional[datetime] = None) -> list[datetime]:
    """The next n free slots.

    A slot is free if nobody already uses that same time on that day and, with days_between > 1,
    there is no upload less than days_between days away (another time on the same day is fine).
    """
    now = now or datetime.now(timezone.utc)
    earliest = now + timedelta(hours=channel.min_lead_hours)
    used = {o.astimezone(channel.tz).replace(second=0, microsecond=0) for o in booked}
    days: set[date] = {u.date() for u in used}
    result: list[datetime] = []
    for slot in _slots(channel, earliest):
        if len(result) >= n:
            break
        if slot <= earliest or slot in used:
            continue
        if channel.days_between > 1 and any(0 < abs((slot.date() - d).days) < channel.days_between for d in days):
            continue
        result.append(slot)
        used.add(slot)
        days.add(slot.date())
    return result
