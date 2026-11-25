# Changelog

All notable changes to AutoUpload. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the project uses [semantic versioning](https://semver.org/).

## [2.1.0] — Unreleased

AI agents can now drive AutoUpload, the desktop app wears the website's look, and the project is Rust from end to end.

### Added
- **MCP server** (`autoupload mcp`): Claude Code, Codex, Cursor and other AI agents get typed tools: `status`,
  `list_queue`, `add_video`, `write_sheet`, `plan`, `upload`, `calendar`, `doctor` and `activity`. Agents may always
  prepare and preview; they upload only while a person allows it (**Agents may upload** in the app, or
  `autoupload agents allow-upload on`), and they can never publish a video as public at once.
- **Shared activity log** (`activity.jsonl` in the data folder): the app, the command line, the daily task and agents
  record what they do. The app's **Activity** tab shows it live, with an "AI · <agent>" badge on agents' actions.
- **AI agents panel** in the app: the upload switch, and the exact lines to register AutoUpload in Claude Code,
  Codex or any MCP client.
- New commands: `queue`, `add`, `sheet`, `activity`, `agents setup|status|allow-upload`, and `--json` output for
  scripts. In a terminal, `AUTOUPLOAD_AGENT=<name>` marks an agent's commands.
- **Demo mode** (`--demo`, for the app and every command): sample channels and an offline YouTube, to try everything
  without a Google account. The new screenshots were taken with it.
- **Daily task in Rust**: `autoupload task install --time 10:00` registers it with Task Scheduler (Windows), launchd
  (macOS) or a systemd user timer (Linux); `task run` uploads every queue, logs to `logs/` (kept 60 days) and records
  the result in the activity log. `task remove` and `task status` too.
- Light and dark mode in the app, with a sun/moon switch that is remembered (`theme` in settings.json).
- Screenshots in the README and on the website, and `AGENTS.md`, a guide for AI agents.

### Changed
- The desktop app follows the website's design: ink masthead with mint tabs, cobalt halo, sheet cards, and the
  Barlow fonts embedded.
- The schedule reads "18:00 every day · Europe/Madrid".
- The website generator is a Rust program (`cargo run -p autoupload-site`); CI checks the published pages are current.
- `autoupload data-folder <path>` keeps the other settings instead of overwriting them.

### Removed
- `install-task.ps1` and `upload-all.ps1`: replaced by `autoupload task`.
- The Python 1.x code, its tests and the Python tooling. Version 1.2.0 remains available from its release.

## [2.0.0] — 2026-11-21

AutoUpload is now a native program written in Rust, with a desktop app, for Windows, macOS and Linux.
Nothing to install first: no Python, no virtual environment.

### Added
- Desktop app (`autoupload-gui`, built with Slint): channels, queue with live upload progress, the channel's
  calendar on YouTube, connect and disconnect, a confirmation before every upload, an option to allow public sheets,
  diagnostics with a fix for each problem, and a button to move the data folder.
- Release downloads for Windows x64, Linux x64, macOS Apple silicon and macOS Intel (an `AutoUpload.app` bundle),
  with SHA-256 checksums and build provenance attestations.
- `autoupload data-folder [path]` shows or sets the data folder; a `channels/` or `config/` folder next to the
  program makes a portable copy.
- Linux `install.sh` adds the binaries to `~/.local/bin` and the app to the applications menu.

### Changed
- The same engine as 1.2 (resumable uploads, no duplicates, retries by API reason, one run per channel, staged
  publishing), rewritten in Rust; it talks to the YouTube Data API over HTTPS with rustls.
- Tokens, `channel.yaml`, video sheets and `state.sqlite` from 1.x are read unchanged, so existing channels keep
  their sign-in and duplicate protection.
- `upload-all.ps1` runs the `autoupload.exe` next to it.

### Fixed
- A publishing hour that does not exist on a daylight-saving day (e.g. 02:30 when clocks jump to 03:00) is moved
  forward instead of being skipped.

### Removed
- The standalone-manifest commands (`validate`, `plan`, `publish`), `migrate`, `studio-package` and `studio-done`.
  They remain in the 1.x Python version, which stays in `src/autoupload/` for now.

## [1.2.0] — 2026-11-21

### Fixed
- An installed copy (`pip install .`) kept `channels/` and `config/` inside the venv (`.venv\Lib\`), so the
  client file was "missing" and a new venv wiped the queues and the duplicate protection. Data now lives in
  `AUTOUPLOAD_HOME`, the repository when running from source, or `Documents\AutoUpload`.
- Publishing hours drifted by one hour at every daylight-saving change.
- Tests used fixed dates and started failing on 2026-11-21.
- A short-term `rateLimitExceeded` stopped the whole run as if the daily quota were spent; errors are now classified
  by their API reason, not by message text.
- A thumbnail rejected because the channel is not verified (403) no longer fails the video: it is reported with a fix.
- A video interrupted after upload (e.g. while adding it to the playlist) is now completed and moved on the next run,
  instead of being reported as a duplicate.

### Added
- `timezone` accepts zone names such as `Europe/Madrid` or `America/Lima` (fixed offsets still work).
- Uploads resume after a crash or a reboot from the last byte Google received (the session is saved in SQLite).
- If an upload reached YouTube but its response was lost, the video is found by file name and size instead of being
  uploaded twice.
- Every API call retries 5xx, 429 and short-term rate limits with backoff, honouring `Retry-After`.
- `autoupload doctor`: offline checks of Python, data folder, OAuth client, sign-in, time zones and ffprobe.
- `autoupload migrate`: moves data left inside the venv by 1.1.0, never overwriting.
- One run at a time per channel (`run.lock`); videos modified in the last 2 minutes wait for the next run.
- Tokens, reports and channel IDs are written atomically; the state database uses WAL and a versioned schema.
- Daily task logs older than 60 days are deleted.
- CI on Windows, macOS and Linux (Python 3.10, 3.13, 3.14) with coverage, an installed-wheel check and a lint job;
  Dependabot; contributing guide, security policy, code of conduct and issue templates.

### Changed
- README and install guide use `pip install -e .`.
- `tzdata` is installed on Windows.

## [1.1.0] — 2026-11-20
- First public release: queue folders, publishing slots per channel, private scheduled upload, thumbnail, captions,
  playlist, staged SQLite state, SHA-256 duplicate protection, channel check, Windows daily task, bilingual website.
