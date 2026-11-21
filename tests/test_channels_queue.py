"""Channels, slots and queue, with a fake YouTube: nothing touches the network."""
from __future__ import annotations

from datetime import datetime, timedelta, timezone

import pytest
import yaml

from autoupload import channels
from autoupload.queue import StopRun, build_manifest, calendar_rows, process_queue, queue_items
from conftest import FakeClient

# Dates are relative to the real clock: the manifest rejects publish_at in the past, so fixed dates expire.
# BASE is "day 20" of the original scenario, a month ahead; at(d, h) is day 20+d at h:00 UTC.
BASE = (datetime.now(timezone.utc) + timedelta(days=30)).replace(hour=0, minute=0, second=0, microsecond=0)


def at(day: int, hour: int) -> datetime:
    return BASE + timedelta(days=day - 20, hours=hour)


def day(x: datetime) -> int:
    return 20 + (x.astimezone(timezone.utc) - BASE).days


NOW = at(20, 12)


class Client(FakeClient):
    def __init__(self, existing=None, channel_id="UC_TEST", **kw):
        super().__init__(**kw)
        self.existing = existing or []
        self.channel_id = channel_id

    def my_channel(self):
        self.calls.append("channels.list")
        return {"id": self.channel_id, "title": "Test channel"}

    def uploads(self, max_items=200):
        self.calls.append("uploads")
        return self.existing


@pytest.fixture
def channel(tmp_path, monkeypatch):
    monkeypatch.setenv("AUTOUPLOAD_HOME", str(tmp_path))
    channels.create_channel("test")
    return channels.load_channel("test")


def put(channel, name, sheet=None, thumb=False):
    (channel.queue / f"{name}.mp4").write_bytes(b"\x00\x00\x00\x18ftypmp42" + name.encode() + b"\x00" * 512)
    if sheet is not None:
        (channel.queue / f"{name}.yaml").write_text(yaml.safe_dump(sheet, allow_unicode=True), encoding="utf-8")
    if thumb:
        (channel.queue / f"{name}.png").write_bytes(b"\x89PNG\r\n\x1a\n" + b"\x00" * 64)


def scheduled(when: datetime) -> dict:
    return {"id": "X", "snippet": {"title": "already there"}, "status": {"privacyStatus": "private",
            "publishAt": when.astimezone(timezone.utc).isoformat().replace("+00:00", "Z")}}


def test_template_is_valid(channel):
    assert channel.hours == ["08:00"] and channel.days_between == 1 and channel.client_secret.name == "client_secret.json"
    assert channels.list_channels() == ["test"]


def test_slots_skip_booked_and_lead_time(channel):
    booked = at(22, 8)
    r = channels.next_slots(channel, 3, [booked], now=NOW)
    assert [day(x) for x in r] == [21, 23, 24]       # the 20th at 08:00 has already passed; the 22nd is taken
    assert all(x.hour == 8 for x in r)


def test_slots_every_other_day(channel):
    c = channel.model_copy(update={"days_between": 2, "hours": ["08:00", "19:00"]})
    r = channels.next_slots(c, 4, [at(21, 8)], now=NOW)
    assert [(day(x), x.hour) for x in r] == [(21, 19), (23, 8), (23, 19), (25, 8)]


def test_sheet_in_english_and_channel_defaults(channel):
    c = channel.model_copy(update={"base_tags": ["channel"], "description_footer": "Subscribe", "contains_ai": True})
    put(c, "My Video 1", {"title": "Hello", "description": "Text", "tags": ["one", "channel"]}, thumb=True)
    item = queue_items(c)[0]
    m = build_manifest(c, item, at(21, 8))
    assert m.id == "my-video-1" and m.title == "Hello" and m.tags == ["channel", "one"]
    assert m.description.endswith("Subscribe") and m.contains_synthetic_media is True
    assert m.privacy.value == "private" and m.publish_at.hour == 8 and m.thumbnail.name == "My Video 1.png"


def test_sheet_with_spanish_key_is_rejected_by_name(channel):
    put(channel, "a", {"titulo": "A"})
    with pytest.raises(ValueError, match="titulo"):
        build_manifest(channel, queue_items(channel)[0], None)


def test_plan_uploads_nothing(channel):
    put(channel, "a", {"title": "A"})
    cli = Client()
    r = process_queue(channel, cli, execute=False, now=NOW, notify=lambda s: None)
    assert len(r.plan) == 1 and "videos.insert" not in cli.calls
    assert (channel.queue / "a.mp4").exists()


def test_schedules_and_moves_to_uploaded(channel):
    put(channel, "a", {"title": "A"})
    put(channel, "b", {"title": "B"})
    put(channel, "c")                                   # no sheet: error, the video stays in the queue
    cli = Client(existing=[scheduled(at(21, 8))])
    r = process_queue(channel, cli, execute=True, now=NOW, notify=lambda s: None)
    assert [h["date"] for h in r.done] == [f"{at(22, 8):%Y-%m-%d %H:%M}", f"{at(23, 8):%Y-%m-%d %H:%M}"]
    assert cli.calls.count("videos.insert") == 2
    assert cli.last_body["status"]["privacyStatus"] == "private" and "publishAt" in cli.last_body["status"]
    assert len(r.errors) == 1 and (channel.queue / "c.mp4").exists()
    assert not (channel.queue / "a.mp4").exists() and len(list(channel.uploaded.glob("*/report.json"))) == 2


def test_wrong_channel_stops(channel):
    channel.channel_id_file.write_text("UC_OTHER", encoding="utf-8")
    put(channel, "a", {"title": "A"})
    with pytest.raises(StopRun):
        process_queue(channel, Client(), execute=True, now=NOW, notify=lambda s: None)
    assert (channel.queue / "a.mp4").exists()


def test_public_requires_permission(channel):
    put(channel, "a", {"title": "A", "privacy": "public"})
    r = process_queue(channel, Client(), execute=True, now=NOW, notify=lambda s: None)
    assert r.errors and not r.done and (channel.queue / "a.mp4").exists()


def test_quota_stops_the_run(channel):
    put(channel, "a", {"title": "A"})
    cli = Client(fail={"videos.insert": [RuntimeError("uploadLimitExceeded")]})
    with pytest.raises(StopRun):
        process_queue(channel, cli, execute=True, now=NOW, notify=lambda s: None)


def test_calendar_rows(channel):
    cli = Client(existing=[scheduled(at(25, 8))])
    rows = calendar_rows(channel, cli, now=NOW)
    assert len(rows) == 1 and rows[0][1] == "private"


def test_same_file_twice_is_not_duplicated(channel):
    put(channel, "a", {"title": "A"})
    (channel.queue / "a2.mp4").write_bytes((channel.queue / "a.mp4").read_bytes())
    (channel.queue / "a2.yaml").write_text("title: Other\n", encoding="utf-8")
    cli = Client()
    r = process_queue(channel, cli, execute=True, now=NOW, notify=lambda s: None)
    assert cli.calls.count("videos.insert") == 1 and len(r.done) == 1
    assert len(r.errors) == 1 and (channel.queue / "a2.mp4").exists()
