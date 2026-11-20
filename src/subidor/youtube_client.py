"""Thin wrapper over the official `youtube` v3 service. All remote calls go through here.

google-api-python-client is imported lazily: the tests use a double and do not need it.
"""
from __future__ import annotations

from pathlib import Path
from typing import Any, Callable, Optional

from .upload import run_resumable

CHUNK = 8 * 1024 * 1024


def _media(path: Path, mimetype: str, resumable: bool):
    from googleapiclient.http import MediaFileUpload  # noqa: WPS433
    return MediaFileUpload(str(path), mimetype=mimetype, chunksize=CHUNK if resumable else -1, resumable=resumable)


class YouTubeClient:
    def __init__(self, service: Any, media_factory: Callable = _media):
        self.s = service
        self.media = media_factory

    @classmethod
    def from_credentials(cls, creds) -> "YouTubeClient":
        from googleapiclient.discovery import build
        return cls(build("youtube", "v3", credentials=creds, cache_discovery=False))

    # --- lectura -------------------------------------------------------------------------------
    def my_channel(self) -> dict:
        r = self.s.channels().list(part="id,snippet", mine=True).execute()
        items = r.get("items", [])
        if not items:
            raise RuntimeError("the authenticated account has no YouTube channel")
        return {"id": items[0]["id"], "title": items[0]["snippet"]["title"]}

    def uploads(self, max_items: int = 200) -> list[dict]:
        """Latest channel videos (also private and scheduled), with snippet and status. Costs about 1 unit per 50."""
        ch = self.s.channels().list(part="contentDetails", mine=True).execute()["items"][0]
        playlist = ch["contentDetails"]["relatedPlaylists"]["uploads"]
        ids: list[str] = []
        token = None
        while len(ids) < max_items:
            r = self.s.playlistItems().list(part="contentDetails", playlistId=playlist, maxResults=50, pageToken=token).execute()
            ids += [i["contentDetails"]["videoId"] for i in r.get("items", [])]
            token = r.get("nextPageToken")
            if not token:
                break
        out: list[dict] = []
        for i in range(0, len(ids), 50):
            out += self.s.videos().list(part="snippet,status", id=",".join(ids[i:i + 50])).execute().get("items", [])
        return out

    def get_video(self, video_id: str, parts: str = "snippet,status,localizations,processingDetails") -> Optional[dict]:
        r = self.s.videos().list(part=parts, id=video_id).execute()
        items = r.get("items", [])
        return items[0] if items else None

    # --- escritura ------------------------------------------------------------------------------
    def insert_video(self, body: dict, part: str, path: Path, notify: bool, on_progress=lambda p: None) -> dict:
        req = self.s.videos().insert(part=part, body=body, notifySubscribers=notify,
                                     media_body=self.media(path, "video/*", True))
        return run_resumable(req, on_progress)

    def update_video(self, body: dict, part: str) -> dict:
        return self.s.videos().update(part=part, body=body).execute()

    def set_thumbnail(self, video_id: str, path: Path) -> dict:
        mime = "image/png" if path.suffix.lower() == ".png" else "image/jpeg"
        return self.s.thumbnails().set(videoId=video_id, media_body=self.media(path, mime, False)).execute()

    def insert_caption(self, video_id: str, language: str, name: str, path: Path) -> dict:
        body = {"snippet": {"videoId": video_id, "language": language, "name": name, "isDraft": False}}
        return self.s.captions().insert(part="snippet", body=body,
                                        media_body=self.media(path, "application/octet-stream", False)).execute()

    def add_to_playlist(self, playlist_id: str, video_id: str) -> dict:
        body = {"snippet": {"playlistId": playlist_id, "resourceId": {"kind": "youtube#video", "videoId": video_id}}}
        return self.s.playlistItems().insert(part="snippet", body=body).execute()
