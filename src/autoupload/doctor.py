"""`autoupload doctor`: offline checks of the installation, with the fix for each problem.

Nothing here touches the network or prints secrets: the client file and the tokens are only checked for shape.
"""
from __future__ import annotations

import json
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Optional

from . import auth, channels
from .validators import find_ffprobe

OK, WARN, ERROR = "ok", "warning", "ERROR"
GUIDE = "https://daosource111.github.io/autoupload/install.html"


@dataclass
class Check:
    level: str
    message: str
    fix: str = ""


def _client_secret_kind(path: Path) -> Optional[str]:
    """'installed' (Desktop app), 'web', or None if the file is not an OAuth client. Only the top-level key is read."""
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None
    if not isinstance(data, dict):
        return None
    return next((k for k in ("installed", "web") if k in data), None)


def _token_has_refresh(path: Path) -> bool:
    try:
        return bool(json.loads(path.read_text(encoding="utf-8")).get("refresh_token"))
    except (OSError, ValueError, AttributeError):
        return False


def _has_tz_database() -> bool:
    try:
        from zoneinfo import ZoneInfo
        ZoneInfo("Europe/Madrid")
        return True
    except Exception:  # noqa: BLE001
        return False


def run(base: Optional[Path] = None) -> list[Check]:
    out: list[Check] = []
    v = sys.version_info
    out.append(Check(OK if v >= (3, 10) else ERROR, f"Python {v.major}.{v.minor}.{v.micro}",
                     "" if v >= (3, 10) else "install Python 3.10 or newer"))

    home = base or channels.root()
    out.append(Check(OK, f"data folder: {home}", ""))
    legacy = None if base else channels.legacy_root()
    if legacy:
        out.append(Check(WARN, f"old data found inside the Python environment: {legacy}",
                         f"move its channels\\ and config\\ folders to {home} (a new venv would delete them)"))

    out.append(Check(OK if find_ffprobe() else WARN, "ffprobe " + ("found" if find_ffprobe() else "not found"),
                     "" if find_ffprobe() else "optional: install FFmpeg so video and caption durations are checked"))

    tz_db = _has_tz_database()
    names = channels.list_channels(base)
    if not names:
        out.append(Check(WARN, "no channels configured", "create one: autoupload channel new <name>"))
    any_token = False
    for n in names:
        try:
            c = channels.load_channel(n, base)
        except (ValueError, FileNotFoundError) as e:
            out.append(Check(ERROR, f"[{n}] invalid channel.yaml", str(e)))
            continue
        if c.timezone[0] not in "+-" and not tz_db:
            out.append(Check(ERROR, f"[{n}] time zone {c.timezone} needs the tz database",
                             "reinstall AutoUpload (it installs tzdata) or: pip install tzdata"))
        kind = _client_secret_kind(c.client_secret) if c.client_secret.exists() else "missing"
        if kind == "missing":
            out.append(Check(ERROR, f"[{n}] OAuth client file not found: {c.client_secret}",
                             f"create a Desktop app OAuth client and save it there; guide: {GUIDE}#client"))
        elif kind == "web":
            out.append(Check(ERROR, f"[{n}] {c.client_secret.name} is a Web application client",
                             "create a client of type Desktop app instead"))
        elif kind is None:
            out.append(Check(ERROR, f"[{n}] {c.client_secret.name} is not an OAuth client file",
                             "download it again from Google Cloud > Clients"))
        else:
            out.append(Check(OK, f"[{n}] OAuth client (Desktop app) found"))
        tok = auth.token_path(c.profile)
        if not tok.exists():
            out.append(Check(WARN, f"[{n}] not connected", f"autoupload auth {n} login"))
        elif not _token_has_refresh(tok):
            out.append(Check(ERROR, f"[{n}] saved sign-in cannot be renewed", f"autoupload auth {n} login"))
        else:
            any_token = True
            linked = c.expected_channel_id()
            out.append(Check(OK, f"[{n}] connected" + (f" to {linked}" if linked else "")))
        queued = len(list(c.queue.iterdir())) if c.queue.exists() else 0
        out.append(Check(OK, f"[{n}] {queued} file(s) in queue, {', '.join(c.hours)} ({c.timezone})"))
    if any_token:
        out.append(Check(OK, "note: if your Google app is in Testing mode, the sign-in expires after 7 days",
                         f"use Production or Personal mode to avoid it; guide: {GUIDE}"))
    return out


def render(checks: list[Check]) -> str:
    mark = {OK: "✓", WARN: "!", ERROR: "✗"}
    lines = []
    for c in checks:
        lines.append(f"{mark[c.level]} {c.message}")
        if c.fix and c.level != OK or c.fix and c.message.startswith("note:"):
            lines.append(f"    → {c.fix}")
    errors = sum(c.level == ERROR for c in checks)
    warnings = sum(c.level == WARN for c in checks)
    lines.append(f"{errors} error(s), {warnings} warning(s)")
    return "\n".join(lines)
