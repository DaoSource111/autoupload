# Known issues
Template: `## Title` · **Symptom** · **Cause** · **File** · **Found** · **Severity**

## 2.0 not yet used for a real upload — MEDIUM (to verify)
**Symptom:** the Rust engine has been checked read-only against real channels (`doctor`, `auth whoami`, `calendar`,
token refresh) and with a fake API in tests, but no video has been uploaded with it yet, and resuming a cut upload
(308 + `Range`, status query `bytes */size`) has not been tried against YouTube.
**File:** crates/core/src/youtube.rs, publisher.rs · **Found:** 2026-11-21

## Python 1.x removed before 2.x made a real upload — MEDIUM (decision, see decisions.md)
**Symptom:** if a real upload with 2.x fails, there is no fallback in the repository; 1.2.0 is still installable
from its release. **Found:** 2026-11-25

## Daily task not yet registered on a real system — MEDIUM (to verify)
**Symptom:** `autoupload task install` (schtasks, launchd, systemd) is covered by unit tests for its pieces only;
it has not been run on Windows, macOS or Linux yet, and `task run` has only run on demo data.
**File:** crates/core/src/task.rs · **Found:** 2026-11-25

## MCP server tried with a scripted client only — LOW (to verify)
**Symptom:** `autoupload mcp` passed a scripted JSON-RPC session (initialize, tools/list, tools/call) in tests and CI,
but has not been registered in Claude Code, Codex or Cursor yet.
**File:** crates/cli/src/mcp.rs · **Found:** 2026-11-25

## Desktop app not yet opened on macOS or Linux — LOW (to verify)
**Symptom:** the Slint app was run on Windows only; CI builds and tests it on all three systems but cannot look at it.
**File:** crates/gui/ · **Found:** 2026-11-21

## Release builds are not code-signed — LOW
**Symptom:** Windows SmartScreen and macOS Gatekeeper warn on first start (the README explains how to open them).
**Cause:** Authenticode and Apple notarization need paid certificates. **File:** .github/workflows/release.yml · **Found:** 2026-11-21

---
## Resolved
- 2026-11-21 · 4 tests expired because of fixed dates → test dates are relative to the real clock.
- 2026-11-21 · `root()` pointed inside the venv → AUTOUPLOAD_HOME, the repository or `Documents\AutoUpload`; `doctor` warns and `migrate` moves the data.
- 2026-11-21 · No daylight saving → IANA zones through `zoneinfo` (+ `tzdata` on Windows).
- 2026-11-21 · "Wrong quota estimate": a false positive of the audit; Google confirms `videos.insert` has its own bucket of 100 calls a day.
- 2026-11-21 · An interrupted upload started from zero → the session is saved and resumed; if the response is lost, the video is found by name and size (`fileDetails`).
- 2026-11-21 · Resuming after a partial failure reported a duplicate → the remaining stages are completed and the file is moved.
- 2026-11-21 · Quota errors detected by message text → classified by `error.reason`; `rateLimitExceeded` no longer stops the run.
- 2026-11-21 · A thumbnail rejected for an unverified channel failed the video → a warning with the fix; the video goes on.
- 2026-11-21 · PRODUCT.md out of date → fixed (local file, not in git).
- 2026-11-21 · Non-existent hour at the spring daylight-saving jump → in 2.0 the slot moves forward past the gap (`Zone::resolve`); still present in the 1.x Python version.
