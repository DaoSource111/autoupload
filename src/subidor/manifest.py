"""Single manifest per video (YAML or JSON), validated with Pydantic.

Safety by default: privacy is `private`. Publishing as `public` also requires the explicit
`--allow-public` permission in the CLI (see publisher.py).
"""
from __future__ import annotations

import json
import re
from datetime import datetime, timezone
from enum import Enum
from pathlib import Path
from typing import Optional

import yaml
from pydantic import BaseModel, Field, field_validator, model_validator

BCP47 = re.compile(r"^[a-z]{2,3}(-[A-Za-z0-9]{2,8})*$")


class Privacy(str, Enum):
    private = "private"
    unlisted = "unlisted"
    public = "public"


class TrackStatus(str, Enum):
    """Own status of each audio track or thumbnail per language, separate from the main video."""
    pending = "pending"            # declared, not validated yet
    prepared = "prepared"          # file validated and packaged for Studio
    uploaded_in_studio = "uploaded_in_studio"  # the user confirmed the manual upload
    verified = "verified"          # checked in the player by a person


class Localization(BaseModel):
    title: str
    description: str = ""

    @field_validator("title")
    @classmethod
    def _title(cls, v: str) -> str:
        return _check_title(v)


class Caption(BaseModel):
    language: str
    name: str = Field(default="", max_length=150)
    path: Path

    @field_validator("language")
    @classmethod
    def _lang(cls, v: str) -> str:
        return _check_lang(v)


class AudioTrack(BaseModel):
    language: str
    path: Path
    status: TrackStatus = TrackStatus.pending

    @field_validator("language")
    @classmethod
    def _lang(cls, v: str) -> str:
        return _check_lang(v)


class LocalizedThumbnail(BaseModel):
    language: str
    path: Path
    status: TrackStatus = TrackStatus.pending


class Manifest(BaseModel):
    id: str = Field(pattern=r"^[a-z0-9][a-z0-9-]{1,60}$")
    video: Path
    thumbnail: Optional[Path] = None
    default_language: str = "es"
    default_audio_language: str = "es"
    title: str
    description: str = ""
    tags: list[str] = []
    category_id: str = "24"  # Entertainment
    privacy: Privacy = Privacy.private
    publish_at: Optional[datetime] = None
    made_for_kids: bool = False
    contains_synthetic_media: Optional[bool] = None
    notify_subscribers: bool = False
    localizations: dict[str, Localization] = {}
    captions: list[Caption] = []
    audio_tracks: list[AudioTrack] = []
    localized_thumbnails: list[LocalizedThumbnail] = []
    playlist_id: Optional[str] = None

    @field_validator("title")
    @classmethod
    def _title(cls, v: str) -> str:
        return _check_title(v)

    @field_validator("description")
    @classmethod
    def _desc(cls, v: str) -> str:
        if len(v.encode("utf-8")) > 5000:
            raise ValueError("description exceeds 5000 bytes")
        if "<" in v or ">" in v:
            raise ValueError("description cannot contain < or >")
        return v

    @field_validator("tags")
    @classmethod
    def _tags(cls, v: list[str]) -> list[str]:
        # YouTube counts tags with spaces as if they had quotes.
        total = sum(len(t) + (2 if " " in t else 0) for t in v) + max(0, len(v) - 1)
        if total > 500:
            raise ValueError(f"tags add up to {total} characters (maximum 500)")
        return v

    @field_validator("default_language", "default_audio_language")
    @classmethod
    def _langs(cls, v: str) -> str:
        return _check_lang(v)

    @model_validator(mode="after")
    def _coherence(self) -> "Manifest":
        for k in self.localizations:
            _check_lang(k)
        if self.default_language in self.localizations:
            raise ValueError("do not repeat the default language inside localizations")
        if self.publish_at is not None:
            if self.privacy != Privacy.private:
                raise ValueError("publish_at requires privacy: private (YouTube publishes it at that time)")
            when = self.publish_at if self.publish_at.tzinfo else self.publish_at.replace(tzinfo=timezone.utc)
            if when <= datetime.now(timezone.utc):
                raise ValueError("publish_at must be in the future")
        seen = set()
        for c in self.captions:
            key = (c.language, c.name)
            if key in seen:
                raise ValueError(f"duplicate caption {key}: YouTube would respond with captionExists")
            seen.add(key)
        return self

    def resolve(self, base: Path) -> "Manifest":
        """Converts relative paths to absolute ones, relative to the manifest folder."""
        def r(p: Optional[Path]) -> Optional[Path]:
            return None if p is None else (p if p.is_absolute() else (base / p).resolve())
        data = self.model_copy(deep=True)
        data.video = r(data.video)
        data.thumbnail = r(data.thumbnail)
        for c in data.captions:
            c.path = r(c.path)
        for a in data.audio_tracks:
            a.path = r(a.path)
        for t in data.localized_thumbnails:
            t.path = r(t.path)
        return data


def _check_title(v: str) -> str:
    if not v.strip():
        raise ValueError("title is empty")
    if len(v) > 100:
        raise ValueError(f"title has {len(v)} characters (maximum 100)")
    if "<" in v or ">" in v:
        raise ValueError("title cannot contain < or >")
    return v


def _check_lang(v: str) -> str:
    if not BCP47.match(v):
        raise ValueError(f"invalid language code: {v!r} (use BCP-47, e.g. es, en, pt-BR)")
    return v


def load_manifest(path: Path) -> Manifest:
    """Reads YAML or JSON and resolves paths relative to the manifest folder."""
    text = path.read_text(encoding="utf-8")
    raw = json.loads(text) if path.suffix.lower() == ".json" else yaml.safe_load(text)
    if not isinstance(raw, dict):
        raise ValueError("the manifest must be an object")
    return Manifest.model_validate(raw).resolve(path.parent)
