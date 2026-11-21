"""Local SQLite state: prevents uploading the same video twice and allows resuming stage by stage.

Schema versions (PRAGMA user_version):
  0 → 1  adds uploads.session (resumable upload session URI) and turns on WAL journaling.
"""
from __future__ import annotations

import hashlib
import sqlite3
from datetime import datetime, timezone
from pathlib import Path
from typing import Optional

STAGES = ["validated", "upload_started", "uploaded", "thumbnail", "captions", "playlist", "verified"]
SCHEMA_VERSION = 1


def file_sha256(path: Path, chunk: int = 4 * 1024 * 1024) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(chunk), b""):
            h.update(block)
    return h.hexdigest()


class State:
    def __init__(self, db: Path):
        db.parent.mkdir(parents=True, exist_ok=True)
        self.con = sqlite3.connect(str(db), timeout=30)
        self.con.execute("PRAGMA journal_mode=WAL")
        self.con.execute("""CREATE TABLE IF NOT EXISTS uploads(
            manifest_id TEXT NOT NULL, sha256 TEXT NOT NULL, channel_id TEXT NOT NULL,
            video_id TEXT, stages TEXT NOT NULL DEFAULT '', updated TEXT NOT NULL,
            PRIMARY KEY(sha256, channel_id))""")
        self.con.execute("""CREATE TABLE IF NOT EXISTS log(
            ts TEXT NOT NULL, manifest_id TEXT, event TEXT NOT NULL, detail TEXT)""")
        self._migrate()
        self.con.commit()

    def _migrate(self) -> None:
        version = self.con.execute("PRAGMA user_version").fetchone()[0]
        if version > SCHEMA_VERSION:
            raise RuntimeError(f"state database is from a newer AutoUpload (schema {version}); update AutoUpload")
        if version < 1:
            cols = {r[1] for r in self.con.execute("PRAGMA table_info(uploads)")}
            if "session" not in cols:
                self.con.execute("ALTER TABLE uploads ADD COLUMN session TEXT")
        self.con.execute(f"PRAGMA user_version={SCHEMA_VERSION}")

    @property
    def schema_version(self) -> int:
        return int(self.con.execute("PRAGMA user_version").fetchone()[0])

    def find(self, sha: str, channel_id: str) -> Optional[dict]:
        row = self.con.execute("SELECT manifest_id, video_id, stages, session FROM uploads WHERE sha256=? AND channel_id=?",
                               (sha, channel_id)).fetchone()
        if not row:
            return None
        return {"manifest_id": row[0], "video_id": row[1], "stages": [s for s in row[2].split(",") if s],
                "session": row[3]}

    def record(self, manifest_id: str, sha: str, channel_id: str, video_id: Optional[str] = None,
               stage: Optional[str] = None, session: Optional[str] = None) -> None:
        cur = self.find(sha, channel_id)
        stages = cur["stages"] if cur else []
        if stage and stage not in stages:
            stages.append(stage)
        vid = video_id or (cur["video_id"] if cur else None)
        sess = session or (cur["session"] if cur else None)
        if vid:
            sess = None  # the upload finished: its session is no longer needed
        self.con.execute("""INSERT INTO uploads(manifest_id, sha256, channel_id, video_id, stages, updated, session)
            VALUES(?,?,?,?,?,?,?) ON CONFLICT(sha256, channel_id) DO UPDATE SET
            manifest_id=excluded.manifest_id, video_id=excluded.video_id, stages=excluded.stages,
            updated=excluded.updated, session=excluded.session""",
                         (manifest_id, sha, channel_id, vid, ",".join(stages), _now(), sess))
        self.con.commit()

    def clear_session(self, sha: str, channel_id: str) -> None:
        self.con.execute("UPDATE uploads SET session=NULL WHERE sha256=? AND channel_id=?", (sha, channel_id))
        self.con.commit()

    def forget(self, sha: str, channel_id: str) -> None:
        self.con.execute("DELETE FROM uploads WHERE sha256=? AND channel_id=?", (sha, channel_id))
        self.con.commit()

    def event(self, manifest_id: str, event: str, detail: str = "") -> None:
        self.con.execute("INSERT INTO log VALUES(?,?,?,?)", (_now(), manifest_id, event, detail))
        self.con.commit()

    def close(self) -> None:
        self.con.close()


def _now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds")
