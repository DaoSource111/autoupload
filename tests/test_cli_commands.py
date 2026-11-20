"""CLI commands: the English commands and options parse, Spanish aliases are rejected by argparse, help is in English. Nothing touches the network."""
from __future__ import annotations

import pytest

from autoupload import channels
from autoupload.cli import main, parse_args


def test_channel_new_parses():
    a = parse_args(["channel", "new", "x"])
    assert a.cmd == "channel" and a.action == "new" and a.name == "x"


@pytest.mark.parametrize("argv, cmd", [
    (["channels"], "channels"),
    (["calendar", "x"], "calendar"),
    (["schedule", "--all"], "schedule"),
    (["validate", "m.yaml"], "validate"),
    (["plan", "m.yaml"], "plan"),
    (["publish", "m.yaml", "--channel", "x"], "publish"),
    (["auth", "x", "login"], "auth"),
    (["studio-package", "m.yaml"], "studio-package"),
    (["studio-done", "m.yaml", "--language", "en"], "studio-done"),
])
def test_each_english_command_parses(argv, cmd):
    assert parse_args(argv).cmd == cmd


def test_schedule_options_parse():
    a = parse_args(["schedule", "x", "--all", "--yes", "--max", "2", "--allow-public"])
    assert a.channel == "x" and a.all is True and a.yes is True and a.max == 2 and a.allow_public is True


def test_schedule_without_flags_defaults():
    a = parse_args(["schedule"])
    assert a.channel is None and a.all is False and a.yes is False and a.allow_public is False


def test_calendar_days_option():
    assert parse_args(["calendar", "x", "--days", "7"]).days == 7
    assert parse_args(["calendar", "x"]).days == 30


def test_publish_options_parse():
    a = parse_args(["publish", "m.yaml", "--channel", "x", "--yes", "--allow-public"])
    assert a.channel == "x" and a.yes is True and a.allow_public is True


@pytest.mark.parametrize("argv", [
    ["programar", "x"],
    ["canal", "nuevo", "x"],
    ["canales"],
    ["calendario", "x"],
    ["validar", "m.yaml"],
    ["publicar", "m.yaml", "--channel", "x"],
    ["schedule", "x", "--todos"],
    ["schedule", "x", "--permitir-publico"],
    ["calendar", "x", "--dias", "7"],
    ["publish", "m.yaml", "--canal", "x"],
])
def test_spanish_alias_is_rejected_by_argparse(argv, capsys):
    with pytest.raises(SystemExit) as exit_info:
        parse_args(argv)
    assert exit_info.value.code == 2
    assert "error" in capsys.readouterr().err


def test_help_is_in_english(capsys):
    with pytest.raises(SystemExit) as exit_info:
        main(["--help"])
    assert exit_info.value.code == 0
    out = capsys.readouterr().out
    assert "Upload and schedule videos" in out and "autoupload" in out
    for command in ("channels", "calendar", "schedule", "validate", "plan", "publish", "studio-package", "studio-done"):
        assert command in out


def test_channels_lists_the_queue(tmp_path, monkeypatch, capsys):
    monkeypatch.setenv("AUTOUPLOAD_HOME", str(tmp_path))
    monkeypatch.setenv("AUTOUPLOAD_CONFIG_DIR", str(tmp_path / "config"))
    channels.create_channel("test")
    assert main(["channels"]) == 0
    out = capsys.readouterr().out
    assert "test" in out and "queue:" in out and "NOT CONNECTED" in out


def test_channel_new_creates_channel_yaml_queue_and_uploaded(tmp_path, monkeypatch, capsys):
    monkeypatch.setenv("AUTOUPLOAD_HOME", str(tmp_path))
    assert main(["channel", "new", "fresh"]) == 0
    folder = tmp_path / "channels" / "fresh"
    assert (folder / "channel.yaml").is_file()
    assert (folder / "queue").is_dir() and (folder / "uploaded").is_dir()
    assert 'timezone: "+00:00"' in (folder / "channel.yaml").read_text(encoding="utf-8")
    assert channels.load_channel("fresh").timezone == "+00:00"
    assert "autoupload auth fresh login" in capsys.readouterr().out
