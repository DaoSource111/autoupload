"""Where channel data lives (root) and named time zones with daylight saving."""
from __future__ import annotations

from datetime import datetime, timezone
from pathlib import Path

import pytest

from autoupload import channels


def _fake_install(monkeypatch, tmp_path: Path) -> Path:
    """Makes channels.py believe it is installed in a venv: <tmp>/venv/Lib/site-packages/autoupload/channels.py."""
    pkg = tmp_path / "venv" / "Lib" / "site-packages" / "autoupload"
    pkg.mkdir(parents=True)
    monkeypatch.setattr(channels, "__file__", str(pkg / "channels.py"))
    monkeypatch.delenv("AUTOUPLOAD_HOME", raising=False)
    monkeypatch.setattr(channels.Path, "home", classmethod(lambda cls: tmp_path / "home"))
    return tmp_path / "venv" / "Lib"


def test_env_overrides_everything(monkeypatch, tmp_path):
    monkeypatch.setenv("AUTOUPLOAD_HOME", str(tmp_path))
    assert channels.root() == tmp_path


def test_source_checkout_uses_the_repository(monkeypatch):
    monkeypatch.delenv("AUTOUPLOAD_HOME", raising=False)
    repo = Path(channels.__file__).resolve().parents[2]
    assert (repo / "pyproject.toml").exists()
    assert channels.root() == repo and channels.legacy_root() is None


def test_installed_copy_never_uses_the_venv(monkeypatch, tmp_path):
    venv_lib = _fake_install(monkeypatch, tmp_path)
    home = channels.root()
    assert home == channels.default_home() and venv_lib not in home.parents and home != venv_lib
    assert home.name == "AutoUpload" and (tmp_path / "home") in home.parents


def test_legacy_data_inside_the_venv_is_detected(monkeypatch, tmp_path):
    venv_lib = _fake_install(monkeypatch, tmp_path)
    assert channels.legacy_root() is None
    (venv_lib / "channels").mkdir()
    assert channels.legacy_root() == venv_lib


def test_bad_timezone_is_rejected(tmp_path, monkeypatch):
    monkeypatch.setenv("AUTOUPLOAD_HOME", str(tmp_path))
    channels.create_channel("tz")
    c = channels.load_channel("tz")
    with pytest.raises(ValueError, match="neither a zone name"):
        channels.Channel.model_validate({**c.model_dump(), "timezone": "Mars/Base"})


def _named(tmp_path, monkeypatch, zone: str):
    pytest.importorskip("zoneinfo")
    try:
        from zoneinfo import ZoneInfo
        ZoneInfo(zone)
    except Exception:  # noqa: BLE001 — no tz database (Windows without tzdata)
        pytest.skip("no time zone database: pip install tzdata")
    monkeypatch.setenv("AUTOUPLOAD_HOME", str(tmp_path))
    channels.create_channel("named")
    c = channels.load_channel("named")
    return c.model_copy(update={"timezone": zone, "hours": ["08:00"]})


def test_named_zone_keeps_local_hour_across_daylight_saving(tmp_path, monkeypatch):
    c = _named(tmp_path, monkeypatch, "Europe/Madrid")
    # Europe moves the clocks back at 03:00 on 2026-10-25: 08:00 Madrid is 06:00 UTC on the 24th, 07:00 UTC from the 25th.
    now = datetime(2026, 10, 23, 12, 0, tzinfo=timezone.utc)
    slots = channels.next_slots(c, 3, [], now=now)
    assert [(s.day, s.hour) for s in slots] == [(24, 8), (25, 8), (26, 8)]
    assert [s.astimezone(timezone.utc).hour for s in slots] == [6, 7, 7]


def test_named_zone_round_trips_booked_dates(tmp_path, monkeypatch):
    c = _named(tmp_path, monkeypatch, "America/Lima")
    booked = channels.booked_from_youtube(
        [{"status": {"privacyStatus": "private", "publishAt": "2026-12-02T13:00:00Z"}, "snippet": {}}], c.tz)
    assert booked[0].hour == 8                      # Lima is UTC-5 all year
    now = datetime(2026, 12, 1, 0, 0, tzinfo=timezone.utc)
    assert [s.day for s in channels.next_slots(c, 2, booked, now=now)] == [1, 3]


def test_migrate_moves_without_overwriting(tmp_path):
    old, new = tmp_path / "venv" / "Lib", tmp_path / "data"
    (old / "channels" / "a").mkdir(parents=True)
    (old / "channels" / "a" / "channel.yaml").write_text("hours: ['08:00']\n")
    (old / "channels" / "b").mkdir()
    (old / "config").mkdir()
    (old / "config" / "client_secret.json").write_text("{}")
    (new / "channels" / "b").mkdir(parents=True)              # already there: must be left alone
    preview = channels.migrate_legacy(execute=False, src=old, dest=new)
    assert any(x.startswith("skip") and "b" in x for x in preview) and (old / "channels" / "a").exists()
    channels.migrate_legacy(execute=True, src=old, dest=new)
    assert (new / "channels" / "a" / "channel.yaml").exists() and (new / "config" / "client_secret.json").exists()
    assert (old / "channels" / "b").exists() and not (old / "channels" / "a").exists()


def test_write_atomic_replaces_whole_file(tmp_path):
    from autoupload.fsutil import write_atomic
    p = tmp_path / "x" / "token.json"
    write_atomic(p, "one")
    write_atomic(p, "two")
    assert p.read_text() == "two" and [f.name for f in p.parent.iterdir()] == ["token.json"]


def test_state_schema_migrates_old_database(tmp_path):
    import sqlite3
    from autoupload.state import SCHEMA_VERSION, State
    db = tmp_path / "state.sqlite"
    con = sqlite3.connect(db)
    con.execute("""CREATE TABLE uploads(manifest_id TEXT NOT NULL, sha256 TEXT NOT NULL, channel_id TEXT NOT NULL,
                   video_id TEXT, stages TEXT NOT NULL DEFAULT '', updated TEXT NOT NULL, PRIMARY KEY(sha256, channel_id))""")
    con.execute("INSERT INTO uploads VALUES('m','s','c','V9','validated,uploaded','2026-11-20')")
    con.commit()
    con.close()
    st = State(db)
    assert st.schema_version == SCHEMA_VERSION
    assert st.find("s", "c") == {"manifest_id": "m", "video_id": "V9", "stages": ["validated", "uploaded"], "session": None}
