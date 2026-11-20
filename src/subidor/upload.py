"""Resumable upload with retries and exponential backoff (official protocol via google-api-python-client)."""
from __future__ import annotations

import logging
import random
import time
from typing import Callable, Optional

log = logging.getLogger(__name__)

RETRIABLE_STATUS = {500, 502, 503, 504}
MAX_RETRIES = 8


class UploadError(RuntimeError):
    pass


def http_status(exc: BaseException) -> Optional[int]:
    """HTTP status of a googleapiclient.errors.HttpError (or of a compatible test double)."""
    resp = getattr(exc, "resp", None)
    status = getattr(resp, "status", None) or getattr(exc, "status_code", None)
    try:
        return int(status) if status is not None else None
    except (TypeError, ValueError):
        return None


def is_retriable(exc: BaseException) -> bool:
    if http_status(exc) in RETRIABLE_STATUS:
        return True
    return isinstance(exc, (ConnectionError, TimeoutError, OSError)) and http_status(exc) is None


def run_resumable(request, on_progress: Callable[[float], None] = lambda p: None,
                  sleep: Callable[[float], None] = time.sleep, max_retries: int = MAX_RETRIES) -> dict:
    """Calls request.next_chunk() until the upload finishes. Retries only recoverable errors."""
    response = None
    retries = 0
    while response is None:
        try:
            status, response = request.next_chunk()
            if status is not None:
                on_progress(float(status.progress()))
            retries = 0
        except Exception as exc:  # noqa: BLE001 — classified below
            if not is_retriable(exc):
                raise UploadError(f"non-recoverable error during upload: {exc}") from exc
            retries += 1
            if retries > max_retries:
                raise UploadError(f"too many retries ({max_retries}): {exc}") from exc
            wait = min(64.0, (2 ** retries) + random.random())
            log.warning("recoverable error (%s); retry %d in %.1fs", http_status(exc) or type(exc).__name__, retries, wait)
            sleep(wait)
    if "id" not in response:
        raise UploadError(f"unexpected response without id: {response}")
    on_progress(1.0)
    return response
