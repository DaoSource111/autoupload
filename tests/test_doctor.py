"""`autoupload doctor`: offline checks, no network, no secrets printed."""
from __future__ import annotations

import json

from autoupload import channels, doctor
from autoupload.cli import main


def _levels(checks, needle):
    return [c.level for c in checks if needle in c.message]


def test_no_channels_is_a_warning(tmp_path, monkeypatch):
    monkeypatch.setenv("AUTOUPLOAD_HOME", str(tmp_path))
    checks = doctor.run()
    assert _levels(checks, "no channels") == [doctor.WARN]
    assert main(["doctor"]) == 0


def test_missing_and_wrong_client_file(tmp_path, monkeypatch):
    monkeypatch.setenv("AUTOUPLOAD_HOME", str(tmp_path))
    monkeypatch.setenv("AUTOUPLOAD_CONFIG_DIR", str(tmp_path / "tokens"))
    channels.create_channel("a")
    assert _levels(doctor.run(), "OAuth client file not found") == [doctor.ERROR]
    assert main(["doctor"]) == 2
    (tmp_path / "config").mkdir()
    (tmp_path / "config" / "client_secret.json").write_text(json.dumps({"web": {"client_secret": "S3CRET"}}))
    assert _levels(doctor.run(), "Web application") == [doctor.ERROR]


def test_healthy_channel_never_prints_secrets(tmp_path, monkeypatch, capsys):
    monkeypatch.setenv("AUTOUPLOAD_HOME", str(tmp_path))
    monkeypatch.setenv("AUTOUPLOAD_CONFIG_DIR", str(tmp_path / "tokens"))
    channels.create_channel("a")
    (tmp_path / "config").mkdir()
    (tmp_path / "config" / "client_secret.json").write_text(json.dumps({"installed": {"client_secret": "S3CRET"}}))
    (tmp_path / "tokens").mkdir()
    (tmp_path / "tokens" / "token-a.json").write_text(json.dumps({"refresh_token": "R3FRESH"}))
    assert main(["doctor"]) == 0
    out = capsys.readouterr().out
    assert "Desktop app" in out and "[a] connected" in out and "7 days" in out
    assert "S3CRET" not in out and "R3FRESH" not in out
