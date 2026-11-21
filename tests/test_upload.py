"""upload.py: error classification by reason, Retry-After, generic retries and resumable sessions. No network."""
from __future__ import annotations

import json

import pytest

from autoupload.upload import (SessionExpired, UploadError, call_with_retry, error_reason, is_retriable,
                               is_stop_error, retry_after, run_resumable)


class ApiError(Exception):
    """Shaped like googleapiclient.errors.HttpError: .resp (dict-like with .status) and .content (JSON bytes)."""

    def __init__(self, status: int, reason: str = "", headers: dict | None = None):
        super().__init__(f"HTTP {status}")
        resp = dict(headers or {})
        self.resp = type("Resp", (dict,), {"status": status})(resp)
        self.content = json.dumps({"error": {"code": status, "errors": [{"reason": reason}] if reason else []}}).encode()


def test_reason_comes_from_the_json_body():
    assert error_reason(ApiError(403, "quotaExceeded")) == "quotaExceeded"
    assert is_stop_error(ApiError(403, "quotaExceeded")) and not is_retriable(ApiError(403, "quotaExceeded"))


def test_reason_found_through_the_exception_chain():
    try:
        try:
            raise ApiError(400, "uploadLimitExceeded")
        except ApiError as inner:
            raise UploadError("wrapped") from inner
    except UploadError as outer:
        assert is_stop_error(outer)


@pytest.mark.parametrize("err, retriable", [
    (ApiError(500), True), (ApiError(503), True), (ApiError(429), True),
    (ApiError(403, "rateLimitExceeded"), True), (ApiError(403, "userRateLimitExceeded"), True),
    (ApiError(403, "forbidden"), False), (ApiError(400), False), (ApiError(404), False),
    (ConnectionError("reset"), True), (TimeoutError(), True),
])
def test_retriable_classification(err, retriable):
    assert is_retriable(err) is retriable


def test_retry_after_header_is_respected():
    waits = []
    calls = iter([ApiError(429, headers={"retry-after": "7"}), {"ok": True}])

    def fn():
        x = next(calls)
        if isinstance(x, Exception):
            raise x
        return x

    assert call_with_retry(fn, sleep=waits.append) == {"ok": True}
    assert waits == [7.0] and retry_after(ApiError(429, headers={"retry-after": "x"})) is None


def test_call_with_retry_gives_up_and_raises_the_original():
    with pytest.raises(ApiError):
        call_with_retry(lambda: (_ for _ in ()).throw(ApiError(503)), sleep=lambda s: None, max_retries=2)


class Chunked:
    """Fake googleapiclient HttpRequest: next_chunk() returns or raises the scripted steps; issues a session URI."""

    def __init__(self, steps, uri="https://upload.example/s/1"):
        self.steps = list(steps)
        self.uri = uri
        self.resumable_uri = None
        self._in_error_state = False
        self.resumed_with = None

    def next_chunk(self):
        if self.resumable_uri and self._in_error_state and self.resumed_with is None:
            self.resumed_with = self.resumable_uri
        if self.resumable_uri is None:
            self.resumable_uri = self.uri
        step = self.steps.pop(0)
        if isinstance(step, Exception):
            raise step
        return step


class Progress:
    def __init__(self, p):
        self.p = p

    def progress(self):
        return self.p


def test_session_uri_is_reported_once_and_upload_finishes():
    seen = []
    req = Chunked([(Progress(0.5), None), (None, {"id": "V1"})])
    assert run_resumable(req, sleep=lambda s: None, on_session=seen.append)["id"] == "V1"
    assert seen == ["https://upload.example/s/1"]


def test_saved_session_is_used_to_resume():
    req = Chunked([(None, {"id": "V2"})])
    run_resumable(req, sleep=lambda s: None, session_uri="https://upload.example/s/old")
    assert req.resumed_with == "https://upload.example/s/old"


def test_gone_session_raises_session_expired():
    with pytest.raises(SessionExpired):
        run_resumable(Chunked([ApiError(404)]), sleep=lambda s: None, session_uri="https://upload.example/s/old")


def test_404_on_a_fresh_upload_is_not_a_session_problem():
    with pytest.raises(UploadError) as e:
        run_resumable(Chunked([ApiError(404)]), sleep=lambda s: None)
    assert not isinstance(e.value, SessionExpired)


def test_daily_limit_during_upload_is_not_retried():
    waits = []
    with pytest.raises(UploadError):
        run_resumable(Chunked([ApiError(403, "uploadLimitExceeded")]), sleep=waits.append)
    assert waits == []
