"""Resumable upload with retries and exponential backoff (official protocol via google-api-python-client).

Errors are classified by their API `reason` (quotaExceeded, rateLimitExceeded, …), not by searching message text.
An upload session can be resumed by a later process: the session URI is handed to `on_session` as soon as Google
issues it, and passing it back as `session_uri` continues from the last byte Google acknowledged.
"""
from __future__ import annotations

import json
import logging
import random
import time
from typing import Callable, Optional

log = logging.getLogger(__name__)

RETRIABLE_STATUS = {429, 500, 502, 503, 504}
RETRIABLE_REASONS = {"rateLimitExceeded", "userRateLimitExceeded", "backendError", "internalError"}
# Daily limits: retrying today is useless, the whole run must stop.
STOP_REASONS = {"quotaExceeded", "dailyLimitExceeded", "uploadLimitExceeded"}
SESSION_GONE = {404, 410}
MAX_RETRIES = 8
MAX_WAIT = 64.0


class UploadError(RuntimeError):
    pass


class SessionExpired(UploadError):
    """The saved resumable session no longer exists at Google: the upload must start again."""


def http_status(exc: BaseException) -> Optional[int]:
    """HTTP status of a googleapiclient.errors.HttpError (or of a compatible test double)."""
    resp = getattr(exc, "resp", None)
    status = getattr(resp, "status", None) or getattr(exc, "status_code", None)
    try:
        return int(status) if status is not None else None
    except (TypeError, ValueError):
        return None


def error_reason(exc: BaseException) -> Optional[str]:
    """The API error reason (e.g. 'quotaExceeded'), from the error details, the JSON body, or as a last resort the text."""
    for e in _chain(exc):
        details = getattr(e, "error_details", None)
        if isinstance(details, list):
            for d in details:
                if isinstance(d, dict) and d.get("reason"):
                    return str(d["reason"])
        content = getattr(e, "content", None)
        if content:
            try:
                err = json.loads(content.decode() if isinstance(content, bytes) else content).get("error", {})
                errors = err.get("errors") or []
                if errors and errors[0].get("reason"):
                    return str(errors[0]["reason"])
            except (ValueError, AttributeError, TypeError):
                pass
        text = str(e)
        for reason in sorted(STOP_REASONS | RETRIABLE_REASONS | {"forbidden", "videoNotFound", "captionExists"}):
            if reason in text:
                return reason
    return None


def _chain(exc: Optional[BaseException]):
    seen = set()
    while exc is not None and id(exc) not in seen:
        seen.add(id(exc))
        yield exc
        exc = exc.__cause__ or exc.__context__


def is_stop_error(exc: BaseException) -> bool:
    return error_reason(exc) in STOP_REASONS


def is_retriable(exc: BaseException) -> bool:
    if is_stop_error(exc):
        return False
    status = http_status(exc)
    if status in RETRIABLE_STATUS:
        return True
    if status == 403 and error_reason(exc) in RETRIABLE_REASONS:
        return True
    return isinstance(exc, (ConnectionError, TimeoutError, OSError)) and status is None


def retry_after(exc: BaseException) -> Optional[float]:
    """Seconds asked by the server in a Retry-After header, if any."""
    resp = getattr(exc, "resp", None)
    value = None
    if resp is not None:
        getter = getattr(resp, "get", None)
        value = getter("retry-after") if callable(getter) else getattr(resp, "retry_after", None)
    try:
        return max(0.0, float(value)) if value is not None else None
    except (TypeError, ValueError):
        return None


def backoff(attempt: int, exc: BaseException) -> float:
    asked = retry_after(exc)
    if asked is not None:
        return min(asked, 300.0)
    return min(MAX_WAIT, (2 ** attempt) + random.random())


def call_with_retry(fn: Callable[[], dict], sleep: Callable[[float], None] = time.sleep,
                    max_retries: int = MAX_RETRIES) -> dict:
    """Runs one API call, retrying only recoverable errors (5xx, 429, short-term rate limits, network)."""
    attempt = 0
    while True:
        try:
            return fn()
        except Exception as exc:  # noqa: BLE001 — classified below
            if not is_retriable(exc) or attempt >= max_retries:
                raise
            attempt += 1
            wait = backoff(attempt, exc)
            log.warning("recoverable error (%s); retry %d in %.1fs", error_reason(exc) or http_status(exc) or type(exc).__name__,
                        attempt, wait)
            sleep(wait)


def run_resumable(request, on_progress: Callable[[float], None] = lambda p: None,
                  sleep: Callable[[float], None] = time.sleep, max_retries: int = MAX_RETRIES, *,
                  session_uri: Optional[str] = None, on_session: Callable[[str], None] = lambda uri: None) -> dict:
    """Calls request.next_chunk() until the upload finishes. Retries only recoverable errors.

    With `session_uri`, it first asks Google how many bytes it already has and continues from there
    (SessionExpired if that session is gone). `on_session` receives the session URI when Google issues it.
    """
    resuming = bool(session_uri)
    if resuming:
        # googleapiclient checks the upload status (PUT with Content-Range: bytes */size) when in error state.
        request.resumable_uri = session_uri
        request._in_error_state = True  # noqa: SLF001 — documented resume path of HttpRequest.next_chunk
    reported = session_uri
    response = None
    retries = 0
    while response is None:
        try:
            status, response = request.next_chunk()
            resuming = False
            if status is not None:
                on_progress(float(status.progress()))
            retries = 0
        except Exception as exc:  # noqa: BLE001 — classified below
            if resuming and http_status(exc) in SESSION_GONE:
                raise SessionExpired(f"the saved upload session expired: {exc}") from exc
            if not is_retriable(exc):
                raise UploadError(f"non-recoverable error during upload: {exc}") from exc
            retries += 1
            if retries > max_retries:
                raise UploadError(f"too many retries ({max_retries}): {exc}") from exc
            wait = backoff(retries, exc)
            log.warning("recoverable error (%s); retry %d in %.1fs", error_reason(exc) or http_status(exc) or type(exc).__name__,
                        retries, wait)
            sleep(wait)
        finally:
            uri = getattr(request, "resumable_uri", None)
            if isinstance(uri, str) and uri != reported:
                reported = uri
                on_session(uri)
    if "id" not in response:
        raise UploadError(f"unexpected response without id: {response}")
    on_progress(1.0)
    return response
