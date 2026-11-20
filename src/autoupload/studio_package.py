"""Package for YouTube Studio: what the public API does not allow (dubbed audio and thumbnails per language).

Uploads nothing. Copies the validated files, writes the exact steps and a checklist with the status per language.
"""
from __future__ import annotations

import json
import shutil
from pathlib import Path
from typing import Optional

from .manifest import Manifest, TrackStatus

STEPS_FILE = "STUDIO-STEPS.md"
STEPS = """# Manual steps in YouTube Studio — {title}

Video: {url}

The public YouTube API (discovery revision 20261022) does not allow uploading dubbed audio tracks or
thumbnails per language. Do it in Studio (requires the channel's advanced features):

## Audio tracks
1. Studio → **Languages** (left menu) → choose the video.
2. **Add language** → choose the language.
3. Next to **Dubbing** → **Add** → **Select file** → choose the file in this folder.
4. **Publish**. If an automatic dub already exists for that language, delete it first.

## Thumbnails per language
1. Studio → **Languages** → language → **Add** next to "Thumbnail" → **Update**.

When you finish each language, mark its status with:
    autoupload studio-done <manifest> --language <code>
"""


def build(m: Manifest, out_dir: Path, video_id: Optional[str]) -> Path:
    out_dir.mkdir(parents=True, exist_ok=True)
    checklist = {"manifest_id": m.id, "video_id": video_id, "audio_tracks": {}, "localized_thumbnails": {}}
    for a in m.audio_tracks:
        dst = out_dir / f"audio-{a.language}{a.path.suffix.lower()}"
        shutil.copy2(a.path, dst)
        state = a.status if a.status != TrackStatus.pending else TrackStatus.prepared
        checklist["audio_tracks"][a.language] = {"file": dst.name, "status": state.value}
    for t in m.localized_thumbnails:
        dst = out_dir / f"thumbnail-{t.language}{t.path.suffix.lower()}"
        shutil.copy2(t.path, dst)
        state = t.status if t.status != TrackStatus.pending else TrackStatus.prepared
        checklist["localized_thumbnails"][t.language] = {"file": dst.name, "status": state.value}
    url = f"https://studio.youtube.com/video/{video_id}/translations" if video_id else "(not uploaded yet)"
    (out_dir / STEPS_FILE).write_text(STEPS.format(title=m.title, url=url), encoding="utf-8")
    (out_dir / "checklist.json").write_text(json.dumps(checklist, ensure_ascii=False, indent=2), encoding="utf-8")
    return out_dir


def mark_done(out_dir: Path, language: str, kind: str = "audio_tracks") -> dict:
    p = out_dir / "checklist.json"
    data = json.loads(p.read_text(encoding="utf-8"))
    if language not in data[kind]:
        raise KeyError(f"{language} is not in {kind}")
    data[kind][language]["status"] = TrackStatus.uploaded_in_studio.value
    p.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")
    return data
