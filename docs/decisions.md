# Architecture decisions
Template: `## YYYY-MM-DD — Title` · **Decision** · **Why** · **Rejected alternatives**

## 2026-11-20 — Official YouTube API only
**Decision:** use the YouTube Data API v3 through OAuth; no browser automation.
**Why:** allowed by the terms of service, stable, and reviewable by Google.
**Rejected:** Playwright or Selenium against Studio (breaks often and violates the terms).

## 2026-11-20 — Fully local, with the user's own OAuth client
**Decision:** no server; each user creates a "Desktop app" OAuth client and uses their own quota.
**Why:** privacy, zero cost, no shared quota.
**Accepted cost:** setting up Google Cloud is hard for non-technical users.

## 2026-11-20 — Safe by default
**Decision:** a plan changes nothing unless `--yes` is given; nothing goes public without `--allow-public`; the token's channel is checked.
**Why:** a mistake uploads real content to a real channel.

## 2026-11-20 — SQLite state keyed by (sha256, channel_id) with stages
**Decision:** record every stage (validated → upload_started → uploaded → thumbnail → captions → playlist → verified).
**Why:** resume without uploading again and never duplicate the same file.

## 2026-11-20 — Strict YAML sheets
**Decision:** unknown keys in `channel.yaml` and in video sheets are errors.
**Why:** a typo is reported instead of silently ignored.

## 2026-11-21 — Data folder outside the installed package
**Decision:** `root()` = `AUTOUPLOAD_HOME` → the repository when running from source (`pyproject.toml` + `src/autoupload`)
→ `Documents\AutoUpload` (`~/AutoUpload` outside Windows). The guide installs with `pip install -e .`, so in the usual
flow (clone and run) the repository stays the data folder.
**Why:** with `pip install .` the data ended up in `.venv\Lib\` and was lost when the venv was recreated.
**Rejected:** `%APPDATA%` (hidden and roaming: bad for large videos); the current directory (depends on where it is launched).

## 2026-11-21 — IANA time zones, offsets still accepted
**Decision:** `timezone` accepts `Europe/Madrid` (through `zoneinfo`) or `+01:00`. `tzdata` is a dependency on Windows only.
**Why:** with fixed offsets the slots moved one hour at every daylight-saving change; existing `channel.yaml` files keep working.

## 2026-11-21 — Resume and duplicate protection without touching metadata
**Decision:** store the resumable session URI in SQLite and, if a response was lost, look for the video among the
latest 50 uploads by `fileDetails.fileName` + `fileSize` (visible only to the owner).
**Why:** prevents duplicates without visible markers in the title, description or tags.
**Accepted risk:** resuming uses googleapiclient's internal path (`resumable_uri`, `_in_error_state`); if the library
changes, it fails with SessionExpired/UploadError and uploads again after searching by file.

## 2026-11-21 — Errors classified by reason; thumbnail 403 is not fatal
**Decision:** classify by `error.reason`; `quotaExceeded`/`uploadLimitExceeded`/`dailyLimitExceeded` stop the run,
`rateLimitExceeded` and 429/5xx are retried. A thumbnail rejected with 403 is a warning.
**Why:** a momentary limit must not stop the day, and an unverified channel must not block the queue forever.

## 2026-11-21 — Developer notes in docs/, kept off the website
**Decision:** `progress.md`, `decisions.md` and `known-issues.md` live in `docs/` and are excluded from GitHub Pages by `docs/_config.yml`.
**Why:** keep the Omega convention without publishing the notes as website pages.

## 2026-11-21 — Releases from a tag through GitHub Actions
**Decision:** pushing `vX.Y.Z` runs `.github/workflows/release.yml`: version check against the tag, tests, wheel + sdist,
SHA256SUMS, build provenance attestation, GitHub release with that version's CHANGELOG section.
**Why:** reproducible releases without local credentials; downloads can be verified.

## 2026-11-21 — English is the primary language
**Decision:** all documentation, code comments, CLI messages, commits and release notes are in English. Spanish exists
only as a translation of user-facing material (`README.es.md`, `docs/_src/es/`), updated after the English source.
**Why:** the project's owner set English as the primary language, always.
