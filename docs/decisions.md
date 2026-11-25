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

## 2026-11-21 — Version 2 in Rust, with a Slint desktop app
**Decision:** rewrite AutoUpload in Rust as a Cargo workspace: `crates/core` (engine), `crates/cli` (`autoupload`)
and `crates/gui` (`autoupload-gui`, Slint with the Fluent style). The YouTube Data API is called over plain HTTPS
(reqwest + rustls), OAuth is our own loopback + PKCE flow, SQLite is bundled.
**Why:** the owner wants native, fast binaries for Windows, macOS and Linux with no runtime to install, and a desktop
app with a polished look; web wrappers (Electron, Tauri's webview) were ruled out. Slint renders natively on all
three systems, ships as one binary and has the most finished widget set among Rust GUI toolkits.
**Compatibility:** the token JSON (google-auth format), `channel.yaml`, sheets and `state.sqlite` schema
(`user_version = 1`) are the same as 1.2, so users switch without signing in again or losing duplicate protection.

## 2026-11-21 — The Python version stays until 2.0 has proven itself
**Decision:** `src/autoupload/` and its tests stay in the repository (CI keeps testing them) until real uploads with
2.0 have succeeded, including a resumed one. Then it is removed in a later release.
**Why:** 2.0 has only been verified read-only against real channels so far.

## 2026-11-21 — Data folder rules in 2.0
**Decision:** `AUTOUPLOAD_HOME` → folder saved in `settings.json` (`autoupload data-folder`, or the app's Change…)
→ `channels/` or `config/` next to the executable (portable) → `Documents\AutoUpload` / `~/AutoUpload`.
**Why:** a compiled program has no "repository folder"; users need one obvious place and a way to move it.

## 2026-11-21 — Per-OS release builds from the tag
**Decision:** `release.yml` builds on native runners: Windows x64 zip, Linux x64 tar.gz (built on Ubuntu 22.04 for
glibc 2.35), macOS arm64 and x64 zips containing an ad-hoc signed `AutoUpload.app`. Checksums, provenance
attestation and CHANGELOG notes as before; the tag must match the workspace version in `Cargo.toml`.
**Why:** one tag produces every download; code signing (Authenticode, Apple notarization) needs paid certificates
and is left for later.

## 2026-11-25 — Rust from end to end; Python 1.x removed
**Decision:** remove `src/autoupload/`, its tests and all Python tooling. The website generator is a Rust program
(`tools/site`, byte-identical output) and the daily task is `autoupload task` instead of PowerShell scripts.
**Why:** the owner wants "100% Rust" to be one of AutoUpload's main advantages; two implementations also doubled
the maintenance. **Accepted risk:** 2.x has not made a real upload yet; 1.2.0 stays installable from its release.

## 2026-11-25 — AI agents through MCP, with a person-held upload switch
**Decision:** `autoupload mcp` is an MCP server (JSON-RPC 2.0 over stdio, written by hand, no SDK) whose tools
call `core::ops`. Agents may read, queue videos, write sheets and preview dates freely; `upload` works only while
`agents_may_upload` is true in settings.json, which only a person can change (app switch or CLI without
`AUTOUPLOAD_AGENT`). `allow_public` is never available to agents. Terminal agents use `--json` and
`AUTOUPLOAD_AGENT=<name>` and get the same rules.
**Why:** MCP is the standard way agents call tools, so one server serves Claude Code, Codex, Cursor and others;
automation should not mean an agent can publish to someone's channel without them deciding it.

## 2026-11-25 — One activity log shared by every actor
**Decision:** append-only `activity.jsonl` in the data folder (time, actor app/cli/agent/task, agent name, channel,
action, summary, ok), rotated at 4 MB. The app polls its modification time every 2 s.
**Why:** the person must be able to see what an agent did, and the agent runs in another process; a plain file in
the data folder needs no server and survives restarts.

## 2026-11-25 — Demo mode with an offline API
**Decision:** `--demo` points the data and config folders at `%TEMP%/AutoUpload-demo` and makes `client::api_for`
return `DemoApi` (in-memory uploads, simulated progress). The app hides local paths in demo mode.
**Why:** people can try every button without a Google account, CI can test the CLI and MCP server end to end, and
screenshots never show real channels, accounts or user paths.

## 2026-11-25 — The desktop app wears the website's design
**Decision:** custom Slint components reproduce `docs/style.css` (tokens for paper and ink, masthead, tabs, sheet
cards, buttons) with Barlow embedded; light/dark follows the system until the person picks one (saved as `theme`).
**Why:** one brand across site and app; the Fluent default looked generic.
