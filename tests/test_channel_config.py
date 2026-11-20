"""channel.yaml: English keys are read, unknown or Spanish keys are rejected by name, and the queue/uploaded folders are fixed."""
from __future__ import annotations

from pathlib import Path

import pytest

from autoupload import channels

ENGLISH = """\
profile: test-en
timezone: "+01:00"
hours: ["19:30", "08:00"]
days_between: 2
min_lead_hours: 1
max_per_run: 3
language: en
category: "28"
made_for_kids: false
contains_ai: true
notify_subscribers: false
playlist: PL123
base_tags: ["ai"]
description_footer: "Thanks"
"""


def write(base: Path, name: str, text: str) -> Path:
    folder = base / "channels" / name
    folder.mkdir(parents=True)
    (folder / "channel.yaml").write_text(text, encoding="utf-8")
    return folder


def test_english_keys_are_read(tmp_path):
    write(tmp_path, "english", ENGLISH)
    c = channels.load_channel("english", base=tmp_path)
    assert c.profile == "test-en" and c.timezone == "+01:00"
    assert c.hours == ["08:00", "19:30"] and c.days_between == 2 and c.min_lead_hours == 1
    assert c.max_per_run == 3 and c.language == "en" and c.category == "28"
    assert c.contains_ai is True and c.notify_subscribers is False and c.made_for_kids is False
    assert c.playlist == "PL123" and c.base_tags == ["ai"] and c.description_footer == "Thanks"


@pytest.mark.parametrize("key, line", [
    ("zona_horaria", 'zona_horaria: "+00:00"\n'),
    ("horas", 'horas: ["08:00"]\n'),
    ("perfil", "perfil: old\n"),
])
def test_spanish_key_is_rejected_by_name(tmp_path, key, line):
    write(tmp_path, "old", line)
    with pytest.raises(ValueError) as error:
        channels.load_channel("old", base=tmp_path)
    assert key in str(error.value)


def test_unknown_key_is_rejected_by_name(tmp_path):
    write(tmp_path, "typo", 'timezne: "+00:00"\n')
    with pytest.raises(ValueError, match="timezne"):
        channels.load_channel("typo", base=tmp_path)


def test_key_set_by_autoupload_cannot_be_written(tmp_path):
    write(tmp_path, "clash", 'folder: /elsewhere\n')
    with pytest.raises(ValueError, match="folder"):
        channels.load_channel("clash", base=tmp_path)


def test_model_defaults_match_the_template(tmp_path):
    c = channels.Channel(name="x", folder=tmp_path, profile="x", client_secret=tmp_path / "s.json")
    assert c.timezone == "+00:00" and c.language == "en" and c.category == "27"
    assert c.hours == ["08:00"]


def test_template_uses_english_keys(tmp_path):
    folder = channels.create_channel("new", base=tmp_path)
    text = (folder / "channel.yaml").read_text(encoding="utf-8")
    assert "profile: new" in text and "hours:" in text
    assert "horas:" not in text and "perfil:" not in text
    assert "autoupload auth new login" in text
    c = channels.load_channel("new", base=tmp_path)
    assert c.profile == "new" and c.hours == ["08:00"] and c.days_between == 1
    assert c.timezone == "+00:00" and c.language == "en"
    assert c.tz.utcoffset(None).total_seconds() == 0


def test_new_channel_uses_queue_and_uploaded(tmp_path):
    channels.create_channel("new", base=tmp_path)
    c = channels.load_channel("new", base=tmp_path)
    assert c.queue == tmp_path / "channels" / "new" / "queue" and c.uploaded == tmp_path / "channels" / "new" / "uploaded"
    assert c.queue.is_dir() and c.uploaded.is_dir()


def test_legacy_folders_are_ignored(tmp_path):
    folder = write(tmp_path, "old", 'hours: ["08:00"]\n')
    (folder / "cola").mkdir()
    (folder / "subidos").mkdir()
    c = channels.load_channel("old", base=tmp_path)
    assert c.queue == folder / "queue" and c.uploaded == folder / "uploaded"
