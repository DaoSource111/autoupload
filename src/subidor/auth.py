"""OAuth 2.0 for desktop applications (loopback + PKCE via google-auth-oauthlib).

- The Google password is never requested: consent happens in the system browser.
- The user downloads client_secret.json from Google Cloud; it is NOT part of the repository.
- The token is stored outside the project, in the user's configuration folder.
"""
from __future__ import annotations

import json
import os
import stat
from pathlib import Path
from typing import Optional

SCOPES = [
    "https://www.googleapis.com/auth/youtube.upload",     # videos.insert, thumbnails.set
    "https://www.googleapis.com/auth/youtube.force-ssl",  # captions.insert, videos.update, playlistItems.insert
    "https://www.googleapis.com/auth/youtube.readonly",   # channels.list, videos.list
]


class AuthError(RuntimeError):
    pass


def config_dir() -> Path:
    base = os.environ.get("SUBIDOR_HOME")
    if base:
        return Path(base)
    root = os.environ.get("APPDATA") or os.environ.get("XDG_CONFIG_HOME") or str(Path.home() / ".config")
    return Path(root) / "subidor-youtube"


def token_path(profile: str = "default") -> Path:
    return config_dir() / f"token-{profile}.json"


def _protect(path: Path) -> None:
    try:
        os.chmod(path, stat.S_IRUSR | stat.S_IWUSR)
    except OSError:
        pass  # On Windows the %APPDATA% folder is already private to the user.


def login(client_secret: Path, profile: str = "default"):
    if not client_secret.exists():
        raise AuthError(f"missing {client_secret}. Create an OAuth client (Desktop app) in Google Cloud and save it there; guide: https://daosource111.github.io/autoupload/install.html#client")
    from google_auth_oauthlib.flow import InstalledAppFlow
    flow = InstalledAppFlow.from_client_secrets_file(str(client_secret), SCOPES)
    creds = flow.run_local_server(port=0, open_browser=True)
    granted = set(creds.scopes or [])
    missing = [s for s in SCOPES if s not in granted]
    if missing:
        raise AuthError(f"not all permissions were granted: {missing}")
    p = token_path(profile)
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(creds.to_json(), encoding="utf-8")
    _protect(p)
    return creds


def load(profile: str = "default"):
    """Saved credentials; refreshes them if they have expired. Raises AuthError if the user must sign in again."""
    p = token_path(profile)
    if not p.exists():
        raise AuthError("no session: run `subidor auth <channel> login`")
    from google.oauth2.credentials import Credentials
    from google.auth.transport.requests import Request
    from google.auth.exceptions import RefreshError
    creds = Credentials.from_authorized_user_info(json.loads(p.read_text(encoding="utf-8")), SCOPES)
    if not creds.valid:
        if creds.expired and creds.refresh_token:
            try:
                creds.refresh(Request())
            except RefreshError as exc:
                raise AuthError("the permission expired or was revoked: run `subidor auth <channel> login` again") from exc
            p.write_text(creds.to_json(), encoding="utf-8")
            _protect(p)
        else:
            raise AuthError("invalid token: run `subidor auth <channel> login` again")
    return creds


def logout(profile: str = "default", revoke: bool = True) -> bool:
    """Revokes the token at Google (if possible) and deletes the local copy."""
    p = token_path(profile)
    if not p.exists():
        return False
    if revoke:
        try:
            import urllib.parse
            import urllib.request
            tok = json.loads(p.read_text(encoding="utf-8")).get("refresh_token") or ""
            data = urllib.parse.urlencode({"token": tok}).encode()
            urllib.request.urlopen("https://oauth2.googleapis.com/revoke", data=data, timeout=15)  # noqa: S310
        except Exception:  # noqa: BLE001 — the local copy is deleted anyway
            pass
    p.unlink()
    return True


def current_profile_token() -> Optional[Path]:
    p = token_path()
    return p if p.exists() else None
