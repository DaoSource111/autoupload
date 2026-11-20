"""Local checks (no network): files, formats, durations and captions."""
from __future__ import annotations

import json
import re
import shutil
import subprocess
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

from .manifest import Manifest

VIDEO_EXT = {".mp4", ".mov", ".mkv", ".webm", ".avi", ".m4v"}
AUDIO_EXT = {".wav", ".mp3", ".m4a", ".aac", ".flac", ".ogg"}
THUMB_EXT = {".jpg", ".jpeg", ".png"}
THUMB_MAX = 50 * 1024 * 1024       # thumbnails.set: 50 MB
CAPTION_MAX = 100 * 1024 * 1024    # captions.insert: 100 MB
AUDIO_TOLERANCE_S = 2.0            # "roughly the same duration": own, conservative margin


@dataclass
class Report:
    errors: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)
    facts: dict = field(default_factory=dict)

    @property
    def ok(self) -> bool:
        return not self.errors


def find_ffprobe(explicit: Optional[str] = None) -> Optional[str]:
    if explicit and Path(explicit).exists():
        return explicit
    return shutil.which("ffprobe")


def probe_duration(path: Path, ffprobe: Optional[str]) -> Optional[float]:
    """Duration in seconds via ffprobe, or None if ffprobe is missing or fails."""
    if not ffprobe:
        return None
    try:
        out = subprocess.run([ffprobe, "-v", "error", "-show_entries", "format=duration", "-of", "json", str(path)],
                             capture_output=True, text=True, timeout=60, check=True).stdout
        return float(json.loads(out)["format"]["duration"])
    except (subprocess.SubprocessError, KeyError, ValueError, OSError):
        return None


_TS = r"(\d{2}):(\d{2}):(\d{2})[,.](\d{3})"
CUE = re.compile(_TS + r"\s*-->\s*" + _TS)


def _secs(h, m, s, ms) -> float:
    return int(h) * 3600 + int(m) * 60 + int(s) + int(ms) / 1000


def validate_caption_text(text: str, kind: str, video_duration: Optional[float] = None) -> list[str]:
    """Errors in an SRT/VTT: header, reversed times, overlaps and cues outside the video."""
    errors = []
    if kind == ".vtt" and not text.lstrip("\ufeff").startswith("WEBVTT"):
        errors.append("VTT is missing the WEBVTT header")
    cues = [(_secs(*m.groups()[:4]), _secs(*m.groups()[4:])) for m in CUE.finditer(text)]
    if not cues:
        errors.append("contains no cue with timings")
    prev_end = 0.0
    for i, (a, b) in enumerate(cues, 1):
        if b <= a:
            errors.append(f"cue {i}: end is not after start")
        if a + 1e-3 < prev_end:
            errors.append(f"cue {i}: overlaps the previous one")
        if video_duration is not None and b > video_duration + 0.5:
            errors.append(f"cue {i}: ends after the video ({b:.2f}s > {video_duration:.2f}s)")
        prev_end = max(prev_end, b)
    return errors


def validate(m: Manifest, ffprobe: Optional[str] = None) -> Report:
    rep = Report()
    fp = find_ffprobe(ffprobe)
    # video
    if not m.video.exists():
        rep.errors.append(f"video file not found: {m.video}")
        vdur = None
    else:
        if m.video.suffix.lower() not in VIDEO_EXT:
            rep.warnings.append(f"unusual video extension: {m.video.suffix}")
        vdur = probe_duration(m.video, fp)
        rep.facts["video_duration"] = vdur
        rep.facts["video_bytes"] = m.video.stat().st_size
        if vdur is None:
            rep.warnings.append("no ffprobe: video duration was not checked")
    # main thumbnail
    if m.thumbnail:
        _check_image(m.thumbnail, "thumbnail", rep)
    for t in m.localized_thumbnails:
        _check_image(t.path, f"thumbnail [{t.language}] (Studio only)", rep)
    # captions
    for c in m.captions:
        if not c.path.exists():
            rep.errors.append(f"caption file not found [{c.language}]: {c.path}")
            continue
        if c.path.stat().st_size > CAPTION_MAX:
            rep.errors.append(f"caption [{c.language}] is over 100 MB")
        kind = c.path.suffix.lower()
        if kind not in {".srt", ".vtt"}:
            rep.warnings.append(f"caption [{c.language}] in {kind}: only SRT and VTT are validated")
            continue
        for e in validate_caption_text(c.path.read_text(encoding="utf-8-sig"), kind, vdur):
            rep.errors.append(f"caption [{c.language}]: {e}")
    # audio tracks (Studio package)
    for a in m.audio_tracks:
        if not a.path.exists():
            rep.errors.append(f"audio track file not found [{a.language}]: {a.path}")
            continue
        if a.path.suffix.lower() not in AUDIO_EXT:
            rep.errors.append(f"audio track [{a.language}] is not audio-only: {a.path.suffix}")
        adur = probe_duration(a.path, fp)
        if adur is not None and vdur is not None:
            diff = abs(adur - vdur)
            rep.facts[f"audio_{a.language}_diff"] = round(diff, 3)
            if diff > AUDIO_TOLERANCE_S:
                rep.errors.append(f"audio track [{a.language}] lasts {adur:.2f}s and the video {vdur:.2f}s (difference {diff:.2f}s)")
        if a.language == m.default_audio_language:
            rep.warnings.append(f"audio track [{a.language}] has the same language as the original audio")
    if m.privacy.value == "public":
        rep.warnings.append("privacy: public — the CLI will require --allow-public")
    return rep


def _check_image(p: Path, label: str, rep: Report) -> None:
    if not p.exists():
        rep.errors.append(f"{label} file not found: {p}")
        return
    if p.suffix.lower() not in THUMB_EXT:
        rep.errors.append(f"{label} must be JPG or PNG")
    if p.stat().st_size > THUMB_MAX:
        rep.errors.append(f"{label} is over 50 MB")
    head = p.read_bytes()[:8]
    if not (head.startswith(b"\xff\xd8") or head.startswith(b"\x89PNG")):
        rep.errors.append(f"{label} is not a valid JPEG/PNG image")
