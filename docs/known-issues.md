# Known issues
Template: `## Title` · **Symptom** · **Cause** · **File** · **Found** · **Severity**

## Real resume not yet tested against YouTube — MEDIUM (to verify)
**Possible symptom:** resuming an upload uses googleapiclient's internal path (`resumable_uri` + `_in_error_state`);
it is covered with test doubles but has not been tried by cutting a real upload.
**File:** src/autoupload/upload.py · **Found:** 2026-11-21

## Non-existent hour at the daylight-saving jump — LOW
**Symptom:** with an IANA zone, a slot inside the spring jump (e.g. 02:30 in Europe/Madrid) is computed with the
offset from before the change. **File:** src/autoupload/channels.py (`_slots`) · **Found:** 2026-11-21

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
