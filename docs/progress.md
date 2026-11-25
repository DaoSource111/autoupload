# Progress — AutoUpload
_Updated: 2026-11-25 · Latest release: v2.1.0_

## Current state
2.1.0 (released 2026-11-25): 100% Rust. Engine (`crates/core`), CLI with an MCP server for AI agents (`crates/cli`),
desktop app styled like the website with light and dark modes (`crates/gui`), website generator (`tools/site`).
57 tests; clippy clean with `-D warnings`; CI runs the CLI and the MCP server end to end in demo mode.
Screenshots in `assets/screenshots/` come from demo mode. No real upload has been made with 2.x yet.

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
- 2026-11-21 · 2.0.0: Rust workspace (core, CLI, Slint GUI) compatible with 1.x tokens, channels and state;
  daylight-saving gap fixed; `data-folder`; Rust CI on 3 systems; per-OS release builds (zip, tar.gz, .app);
  Linux installer and menu entry; `upload-all.ps1` runs the bundled CLI.

- 2026-11-25 · 2.1.0: MCP server and agent permission; shared activity log shown in the app; AI agents panel;
  `queue`/`add`/`sheet`/`activity`/`agents`/`task` commands and `--json`; demo mode; daily task in Rust; website-style
  app with light/dark; Rust site generator; Python removed; screenshots; AGENTS.md.

## Pending (by system · [P1]–[P4] = phase)

### Quality (tests, CI, packaging)
- [P1] Set a coverage threshold from the real CI figure.
- [P2] proptest for `next_slots`; cargo-deny/audit in CI; gitleaks.
- [P2] Optional integration test against a test channel (including cutting a real upload and resuming it).
- [P1] First real upload with 2.x, including cutting one and resuming it.
- [P1] Register `autoupload mcp` in Claude Code and Codex and run a real agent session on demo data.
- [P1] Run `autoupload task install` on Windows, macOS and Linux.
- [P3] Code signing (Authenticode, Apple notarization); winget, Homebrew cask and Flatpak/AppImage.
- [P3] Windows installer (MSI) with a Start menu entry.

### Channels and calendar
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
- [P3] Token in the system credential store (`keyring` crate); check whether the 3 scopes are minimal.

### CLI and scheduled run
- [P2] `doctor --online`: check that the API is enabled and really refresh the token.
- [P2] JSON logs; `--quiet` (rotation is done).
- [P3] Notifications (Windows toast and webhook); CLI in English and Spanish with gettext.
- [P3] Desktop app: edit `channel.yaml` and video sheets in forms; drag and drop videos into the queue; tray icon.
- [P4] Optional AI metadata (user's own key, off by default).

### Public website and brand
- [P3] GIF demo, FAQ and troubleshooting page.
- [P3] Full reference with MkDocs; an honest comparison page with other tools.
- [P4] Outreach (r/NewTubers, Show HN) and fast answers to issues.
