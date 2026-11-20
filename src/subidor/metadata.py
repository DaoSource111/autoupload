"""Request bodies for videos.insert / videos.update, and safe merging of parts."""
from __future__ import annotations

import copy
from datetime import timezone

from .manifest import Manifest


def localizations_body(m: Manifest) -> dict:
    return {lang: {"title": loc.title, "description": loc.description} for lang, loc in m.localizations.items()}


def insert_body(m: Manifest) -> tuple[dict, str]:
    """Returns (body, part). Privacy is always set explicitly: the API uses `public` when it is omitted."""
    snippet = {
        "title": m.title,
        "description": m.description,
        "tags": m.tags,
        "categoryId": m.category_id,
        "defaultLanguage": m.default_language,
        "defaultAudioLanguage": m.default_audio_language,
    }
    status = {"privacyStatus": m.privacy.value, "selfDeclaredMadeForKids": m.made_for_kids}
    if m.publish_at:
        when = m.publish_at if m.publish_at.tzinfo else m.publish_at.replace(tzinfo=timezone.utc)
        status["publishAt"] = when.astimezone(timezone.utc).isoformat().replace("+00:00", "Z")
    if m.contains_synthetic_media is not None:
        status["containsSyntheticMedia"] = m.contains_synthetic_media
    body = {"snippet": snippet, "status": status}
    parts = ["snippet", "status"]
    if m.localizations:
        body["localizations"] = localizations_body(m)
        parts.append("localizations")
    return body, ",".join(parts)


# Read-only fields the API returns; they must not be sent back in an update.
READ_ONLY = {
    "snippet": {"publishedAt", "channelId", "thumbnails", "channelTitle", "liveBroadcastContent", "localized"},
    "status": {"uploadStatus", "failureReason", "rejectionReason", "madeForKids"},
}


def merge_for_update(current: dict, changes: dict, parts: list[str]) -> dict:
    """videos.update overwrites each full part: start from the current resource so no fields are lost."""
    body = {"id": current["id"]}
    for part in parts:
        base = copy.deepcopy(current.get(part, {}))
        for k in READ_ONLY.get(part, set()):
            base.pop(k, None)
        if part == "localizations":
            base.update(changes.get(part, {}))
        else:
            base.update(changes.get(part, {}))
        body[part] = base
    if "snippet" in body and ("title" not in body["snippet"] or "categoryId" not in body["snippet"]):
        raise ValueError("videos.update of snippet requires title and categoryId")
    return body
