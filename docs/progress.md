# Progress — AutoUpload
_Updated: 2026-11-21 · Latest release: v1.2.0 · Audit score: 6.5/10 at v1.1.0, about 8.5/10 now_

## Current state
v1.2.0 released on 2026-11-21 (GitHub release with wheel, sdist, SHA256SUMS and build provenance attestation).
Phases 1 and 2 are done. 130 tests; CI green on 2026-11-21 (11 jobs: lint, 9 OS × Python combinations, installed wheel).
What stands between this and 10/10 is mostly real-world use (maturity) and ease of use for non-technical creators.

## Done
- v1.1.0: queue folders, publishing slots per channel, private scheduled upload, thumbnail, captions, playlist,
  staged SQLite state, SHA-256 duplicate protection, token channel check, Windows daily task.
- Public website in English and Spanish, and brand assets.
- 2026-11-21 · Omega setup (CLAUDE.md and docs/).
- 2026-11-21 · Phase 1: tests without fixed dates; data folder outside the venv; IANA time zones; `doctor`; CI;
  install guide with `pip install -e .`.
- 2026-11-21 · Phase 2 (v1.2.0): uploads resumable across runs; lost responses recovered through `fileDetails`;
  retries with `Retry-After` on every call; errors classified by `reason`; thumbnail 403 is not fatal; resuming
  completes the remaining stages; per-channel lock; waiting for files still copying; atomic writes (token included);
  SQLite WAL and versioned schema; `migrate`; log rotation; CI with lint and coverage; Dependabot; CHANGELOG,
  CONTRIBUTING, SECURITY, CODE_OF_CONDUCT, issue and PR templates.
- 2026-11-21 · Release workflow: a `v*` tag builds, tests, attests and publishes the GitHub release from CHANGELOG.md.
- 2026-11-21 · Rule: English is the primary language; all documentation is written in English.

## Pending (by system · [P1]–[P4] = phase)

### Quality (tests, CI, packaging)
- [P1] Set a coverage threshold from the real CI figure.
- [P2] mypy --strict, pre-commit (full ruff, gitleaks), lockfile.
- [P2] Hypothesis for `next_slots`.
- [P2] Optional integration test against a test channel (including cutting a real upload and resuming it).
- [P3] Publish to PyPI from the release workflow (needs the PyPI project and its trusted publisher) and to winget.
- [P3] Windows installer `.exe` (PyInstaller or Nuitka).

### Channels and calendar
- [P2] Non-existent hours at the spring daylight-saving jump (see known-issues).
- [P4] Export the calendar to `.ics`; reschedule or cancel already scheduled videos.
- [P4] Separate rules for Shorts.

### Publishing and state
- [P2] Stop before the quota runs out (warn if the plan exceeds the remaining units).
- [P4] Share the quota across several channels.

### YouTube client and upload
- [P2] Unverified channels: warn before uploading videos longer than 15 minutes (thumbnails are already handled).

### Queue
- [P3] Optional sheet (title = file name); templates with variables.
- [P4] Watch mode for the `queue/` folder.

### OAuth authentication
- [P3] Token in the system credential store (`keyring`); check whether the 3 scopes are minimal.

### CLI and scheduled run
- [P2] `doctor --online`: check that the API is enabled and really refresh the token.
- [P2] JSON logs; `--quiet` (rotation is done).
- [P3] `autoupload init` (wizard) and a cross-platform `install-schedule` replacing the `.ps1` scripts.
- [P3] Notifications (Windows toast and webhook); CLI in English and Spanish with gettext.
- [P4] Minimal GUI (tray or local app).
- [P4] Optional AI metadata (user's own key, off by default).

### Public website and brand
- [P3] GIF demo, FAQ and troubleshooting page.
- [P3] Full reference with MkDocs; an honest comparison page with other tools.
- [P4] Outreach (r/NewTubers, Show HN) and fast answers to issues.
