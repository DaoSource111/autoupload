"""CLI errors: a missing or broken channel config prints a short English message on stderr and returns 2, without a traceback."""
from __future__ import annotations

import pytest

from subidor import canales
from subidor.cli import INVALIDO, main


def test_missing_channel_is_a_clean_error(tmp_path, monkeypatch, capsys):
    monkeypatch.setenv("SUBIDOR_RAIZ", str(tmp_path))
    assert main(["calendar", "nope"]) == INVALIDO
    err = capsys.readouterr().err
    assert "Error: channel «nope» does not exist" in err and "Traceback" not in err


def test_duplicate_key_is_a_clean_error(tmp_path, monkeypatch, capsys):
    monkeypatch.setenv("SUBIDOR_RAIZ", str(tmp_path))
    canales.crear("dup")
    (tmp_path / "canales" / "dup" / "canal.yaml").write_text('horas: ["08:00"]\nhours: ["19:00"]\n', encoding="utf-8")
    assert main(["schedule", "dup"]) == INVALIDO
    err = capsys.readouterr().err
    assert "keep only one" in err and "Traceback" not in err


def test_verbose_keeps_the_traceback(tmp_path, monkeypatch):
    monkeypatch.setenv("SUBIDOR_RAIZ", str(tmp_path))
    with pytest.raises(FileNotFoundError):
        main(["-v", "calendar", "nope"])
