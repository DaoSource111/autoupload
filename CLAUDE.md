# AutoUpload

YouTube upload scheduler for Windows: each video in `channels/<channel>/queue/`, with its YAML sheet, is uploaded as
private and scheduled in the channel's next free publishing slot (YouTube Data API v3).
Fully local: no server, the user's own OAuth client and quota. Author: Ellipsio Studio.

**Stack:** Python ≥3.10 · google-api-python-client, google-auth-oauthlib · pydantic v2 · PyYAML · SQLite · pytest.
Public website: static HTML/CSS in `docs/` (GitHub Pages), generated with `tools/build_site.py`.

## Language — always English
- **English is the project's primary language. All documentation is written in English, always**: CLAUDE.md,
  `docs/*.md` (progress, decisions, known issues), README.md, CHANGELOG, CONTRIBUTING, SECURITY, code comments,
  docstrings, CLI messages, commit messages, PR descriptions and release notes.
- Spanish exists only as a translation of user-facing material (`README.es.md`, `docs/_src/es/`). English is the
  source of truth: write or change the English text first, then update the Spanish translation to match.
- This applies even when the conversation with the user is in Spanish.

## Run and test
```powershell
py -m venv .venv ; .\.venv\Scripts\pip install -e .[test]
.\.venv\Scripts\autoupload.exe doctor                    # offline setup check, shows the data folder
.\.venv\Scripts\autoupload.exe channels                  # channels and their queues
.\.venv\Scripts\autoupload.exe schedule <channel>        # plan only, uploads nothing
.\.venv\Scripts\autoupload.exe schedule <channel> --yes  # real upload (real channel!)
py -3 -m pytest -q                                       # tests, no network (FakeClient)
py -3 tools/build_site.py                                # rebuild docs/ from docs/_src/
```

Never run `schedule --yes`, `publish --yes` or `auth … login` unless the user asks: they act on real channels.

Releases: bump `version` in `pyproject.toml` and `src/autoupload/__init__.py`, date the section in `CHANGELOG.md`,
push, then push a tag `vX.Y.Z`; `.github/workflows/release.yml` builds, attests and publishes the GitHub release.

## Folder map
| Folder | Contents |
|---|---|
| `src/autoupload/` | the application (installable package, `autoupload` CLI) |
| `tests/` | pytest; `conftest.py` holds `FakeClient`, a fake YouTube |
| `channels/` | local per-channel data: `channel.yaml`, `queue/`, `uploaded/`, `state.sqlite`, `channel_id.txt` (not in git) |
| `config/` | the user's `client_secret.json` (not in git; never read or print it) |
| `docs/` | public website on GitHub Pages; **edit `docs/_src/{en,es}/`**, not the generated HTML. `progress.md`, `decisions.md` and `known-issues.md` are excluded from the site by `docs/_config.yml` |
| `tools/` | `build_site.py` (website generator) |
| `assets/brand/` | logos and social preview |
| root | `install-task.ps1`, `upload-all.ps1` (Windows daily task), `pyproject.toml` |

## Systems
| System | Main files |
|---|---|
| OAuth authentication | `auth.py` |
| YouTube client and resumable upload | `youtube_client.py`, `upload.py` |
| Staged publishing and state | `publisher.py`, `state.py` (versioned schema, `PRAGMA user_version`), `metadata.py`, `fsutil.py` |
| Channels, data folder and calendar | `channels.py` (`root()`, `migrate_legacy`, IANA zones) |
| Video queue | `queue.py` (per-channel lock, waits for files still copying) |
| Manifest and validation | `manifest.py`, `validators.py`, `studio_package.py` |
| CLI, diagnostics and scheduled run | `cli.py`, `doctor.py`, `install-task.ps1`, `upload-all.ps1` |
| Public website and brand | `docs/_src/`, `tools/build_site.py`, `assets/brand/` |
| Quality and community | `tests/`, `pyproject.toml`, `.github/` (CI, release, Dependabot, templates), `CHANGELOG.md`, `CONTRIBUTING.md`, `SECURITY.md` |

(The `.py` files live in `src/autoupload/`.)

## Conventions
- `from __future__ import annotations` and type hints on every signature; pydantic with `extra="forbid"` (unknown keys are errors).
- Google libraries are imported inside functions, so the tests do not need them.
- Every remote call goes through `YouTubeClient`; tests use `FakeClient`, never the network.
- Safe by default: nothing is uploaded without `--yes`, nothing goes public without `--allow-public`, and the token must belong to the expected channel.
- CLI exit codes: `OK=0, INVALID=2, AUTH=3, REMOTE=4`.
- Small, descriptive commits, with `Co-Authored-By` when Claude contributed.
