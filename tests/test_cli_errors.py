"""CLI errors: a missing or broken channel config prints a short English message on stderr and returns 2, without a traceback."""
from __future__ import annotations

import pytest

from autoupload import channels
from autoupload.cli import INVALID, main


def test_missing_channel_is_a_clean_error(tmp_path, monkeypatch, capsys):
    monkeypatch.setenv("AUTOUPLOAD_HOME", str(tmp_path))
    assert main(["calendar", "nope"]) == INVALID
    err = capsys.readouterr().err
    assert "Error: channel 'nope' does not exist" in err and "Traceback" not in err


def test_spanish_key_in_channel_yaml_is_a_clean_error(tmp_path, monkeypatch, capsys):
    monkeypatch.setenv("AUTOUPLOAD_HOME", str(tmp_path))
    channels.create_channel("old")
    (tmp_path / "channels" / "old" / "channel.yaml").write_text('zona_horaria: "+00:00"\n', encoding="utf-8")
    assert main(["schedule", "old"]) == INVALID
    err = capsys.readouterr().err
    assert "zona_horaria" in err and "Traceback" not in err


def test_verbose_keeps_the_traceback(tmp_path, monkeypatch):
    monkeypatch.setenv("AUTOUPLOAD_HOME", str(tmp_path))
    with pytest.raises(FileNotFoundError):
        main(["-v", "calendar", "nope"])
