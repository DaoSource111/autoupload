"""CLI aliases: the English command or option and its Spanish alias reach the same command. Nothing touches the network."""
from __future__ import annotations

import pytest

from subidor import canales
from subidor.cli import main, parse_args


def test_channel_new_english_and_spanish_are_the_same():
    en = parse_args(["channel", "new", "x"])
    es = parse_args(["canal", "nuevo", "x"])
    assert en.cmd == es.cmd == "channel"
    assert en.action == es.action == "new"
    assert en.name == es.name == "x"


@pytest.mark.parametrize("argv, cmd", [
    (["channels"], "channels"), (["canales"], "channels"),
    (["calendar", "x"], "calendar"), (["calendario", "x"], "calendar"),
    (["schedule", "--all"], "schedule"), (["programar", "--todos"], "schedule"),
    (["validate", "m.yaml"], "validate"), (["validar", "m.yaml"], "validate"),
    (["plan", "m.yaml"], "plan"),
    (["publish", "m.yaml", "--channel", "x"], "publish"), (["publicar", "m.yaml", "--canal", "x"], "publish"),
    (["auth", "x", "login"], "auth"),
    (["studio-package", "m.yaml"], "studio-package"),
    (["studio-done", "m.yaml", "--language", "en"], "studio-done"),
])
def test_each_alias_maps_to_its_command(argv, cmd):
    assert parse_args(argv).cmd == cmd


def test_schedule_options_english_and_spanish_match():
    en = parse_args(["schedule", "x", "--all", "--yes", "--max", "2", "--allow-public"])
    es = parse_args(["programar", "x", "--todos", "--yes", "--max", "2", "--permitir-publico"])
    for a in (en, es):
        assert a.channel == "x" and a.all is True and a.yes is True and a.max == 2 and a.allow_public is True


def test_schedule_without_flags_defaults():
    a = parse_args(["schedule"])
    assert a.channel is None and a.all is False and a.yes is False and a.allow_public is False


def test_calendar_days_english_and_spanish_match():
    assert parse_args(["calendar", "x", "--days", "7"]).days == 7
    assert parse_args(["calendario", "x", "--dias", "7"]).days == 7
    assert parse_args(["calendar", "x"]).days == 30


def test_publish_channel_english_and_spanish_match():
    en = parse_args(["publish", "m.yaml", "--channel", "x", "--yes", "--allow-public"])
    es = parse_args(["publicar", "m.yaml", "--canal", "x", "--yes", "--permitir-publico"])
    for a in (en, es):
        assert a.channel == "x" and a.yes is True and a.allow_public is True


def test_help_is_in_english(capsys):
    with pytest.raises(SystemExit) as salida:
        main(["--help"])
    assert salida.value.code == 0
    out = capsys.readouterr().out
    assert "Upload and schedule videos" in out and "channels" in out


def test_channels_and_canales_print_the_same(tmp_path, monkeypatch, capsys):
    monkeypatch.setenv("SUBIDOR_RAIZ", str(tmp_path))
    monkeypatch.setenv("SUBIDOR_HOME", str(tmp_path / "config"))
    canales.crear("prueba")
    assert main(["channels"]) == 0
    en = capsys.readouterr().out
    assert main(["canales"]) == 0
    es = capsys.readouterr().out
    assert en == es
    assert "prueba" in en and "queue:" in en and "NOT CONNECTED" in en
