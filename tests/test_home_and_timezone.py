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
    # Europe moves the clocks back on 2026-10-25: 08:00 Madrid is 06:00 UTC before and 07:00 UTC after.
    now = datetime(2026, 10, 23, 12, 0, tzinfo=timezone.utc)
    slots = channels.next_slots(c, 3, [], now=now)
    assert [s.hour for s in slots] == [8, 8, 8]
    assert [s.astimezone(timezone.utc).hour for s in slots] == [6, 6, 7]


def test_named_zone_round_trips_booked_dates(tmp_path, monkeypatch):
    c = _named(tmp_path, monkeypatch, "America/Lima")
    booked = channels.booked_from_youtube(
        [{"status": {"privacyStatus": "private", "publishAt": "2026-12-02T13:00:00Z"}, "snippet": {}}], c.tz)
    assert booked[0].hour == 8                      # Lima is UTC-5 all year
    now = datetime(2026, 12, 1, 0, 0, tzinfo=timezone.utc)
    assert [s.day for s in channels.next_slots(c, 2, booked, now=now)] == [1, 3]
