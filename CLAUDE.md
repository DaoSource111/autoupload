# AutoUpload

YouTube upload scheduler for Windows, macOS and Linux (desktop app + CLI): each video in `channels/<channel>/queue/`,
with its YAML sheet, is uploaded as private and scheduled in the channel's next free publishing slot (YouTube Data API v3).
Fully local: no server; each user brings their own OAuth client and quota. Author: Ellipsio Studio.

**Stack (2.x):** Rust ≥1.80 Cargo workspace · reqwest (blocking, rustls) · rusqlite (bundled) · chrono + chrono-tz ·
serde_yaml · clap · Slint 1.x (desktop app, Fluent style).
**Legacy (1.x, kept until 2.0 has done real uploads):** Python ≥3.10 in `src/autoupload/`, pytest in `tests/`.
Public website: static HTML/CSS in `docs/` (GitHub Pages), generated with `tools/build_site.py`.

## Language — always English
- **English is the project's primary language. All documentation is written in English, always**: CLAUDE.md,
  `docs/*.md` (progress, decisions, known issues), README.md, CHANGELOG, CONTRIBUTING, SECURITY, code comments,
  docstrings, CLI messages, commit messages, PR descriptions and release notes.
- Spanish exists only as a translation of user-facing material (`README.es.md`, `docs/_src/es/`). English is the
  source of truth: write or change the English text first, then update the Spanish translation to match.
- This applies even when the conversation with the user is in Spanish.

## Run and test
Rust is installed in `D:\DevTools` on this machine; in Bash first run
`export PATH="/d/DevTools/cargo/bin:$PATH" RUSTUP_HOME='D:\DevTools\rustup' CARGO_HOME='D:\DevTools\cargo'`.
```powershell
cargo test --workspace                                   # tests, no network (FakeApi)
cargo clippy --workspace --all-targets -- -D warnings ; cargo fmt --all
cargo run -p autoupload -- doctor                        # offline setup check, shows the data folder
cargo run -p autoupload -- schedule <channel>            # plan only, uploads nothing
cargo run -p autoupload -- schedule <channel> --yes      # real upload (real channel!)
cargo run -p autoupload-gui                              # desktop app; AUTOUPLOAD_HOME=E:\autoupload for the real data
py -3 -m pytest -q                                       # legacy Python tests
py -3 tools/build_site.py                                # rebuild docs/ from docs/_src/
```

Never run `schedule --yes`, `publish --yes`, `auth … login` or the app's Upload/Connect buttons unless the user asks:
they act on real channels.

Releases: bump `version` in the root `Cargo.toml` (`[workspace.package]`) and run `cargo build` so `Cargo.lock`
follows, date the section in `CHANGELOG.md`, push, then push a tag `vX.Y.Z`; `.github/workflows/release.yml` tests,
builds the Windows, Linux and macOS packages, attests them and publishes the GitHub release.

## Folder map
| Folder | Contents |
|---|---|
| `crates/core/` | engine library `autoupload-core`; tests inline, `publisher::fake::FakeApi` is the fake YouTube |
| `crates/cli/` | `autoupload` command line (clap) |
| `crates/gui/` | `autoupload-gui` desktop app: `ui/app.slint`, `src/main.rs`, `build.rs` (Windows icon) |
| `packaging/` | Linux `.desktop` + `install.sh`, macOS `Info.plist` template |
| `src/autoupload/`, `tests/` | legacy Python 1.x and its pytest suite (`conftest.py` holds `FakeClient`) |
| `channels/` | local per-channel data: `channel.yaml`, `queue/`, `uploaded/`, `state.sqlite`, `channel_id.txt` (not in git) |
| `config/` | the user's `client_secret.json` (not in git; never read or print it) |
| `docs/` | public website on GitHub Pages; **edit `docs/_src/{en,es}/`**, not the generated HTML. `progress.md`, `decisions.md` and `known-issues.md` are excluded from the site by `docs/_config.yml` |
| `tools/` | `build_site.py` (website generator) |
| `assets/brand/` | logos and social preview |
| root | `Cargo.toml` (workspace, version), `install-task.ps1`, `upload-all.ps1` (Windows daily task), `pyproject.toml` (legacy) |

## Systems
| System | Main files (under `crates/`) |
|---|---|
| OAuth authentication | `core/src/auth.rs` (loopback + PKCE, google-auth token format) |
| YouTube client and resumable upload | `core/src/youtube.rs` (`Api` trait, `YouTube`, retries, 8 MiB chunks) |
| Staged publishing and state | `core/src/publisher.rs`, `state.rs` (schema `user_version = 1`), `fsutil.rs` |
| Channels, data folder and calendar | `core/src/channel.rs` (zones, `next_slots`), `paths.rs` (data root, settings) |
| Video queue and runs | `core/src/queue.rs` (lock, files still copying, manifest build), `runner.rs` (events for CLI and GUI) |
| Manifest and validation | `core/src/manifest.rs`, `validate.rs` |
| CLI, diagnostics and scheduled run | `cli/src/main.rs`, `core/src/doctor.rs`, `install-task.ps1`, `upload-all.ps1` |
| Desktop app | `gui/ui/app.slint`, `gui/src/main.rs` |
| Public website and brand | `docs/_src/`, `tools/build_site.py`, `assets/brand/` |
| Quality, packaging and community | `.github/` (CI, release, Dependabot, templates), `packaging/`, `CHANGELOG.md`, `CONTRIBUTING.md`, `SECURITY.md` |

## Conventions
- Rust: `deny_unknown_fields` on config structs (unknown keys are errors); clippy clean with `-D warnings`; `cargo fmt`.
- Every remote call goes through the `Api` trait; tests use `FakeApi`, never the network.
- 1.x compatibility: never change the token JSON, `channel.yaml` keys or the SQLite schema without a migration.
- Safe by default: nothing is uploaded without `--yes`, nothing goes public without `--allow-public`, and the token must belong to the expected channel.
- CLI exit codes: `OK=0, INVALID=2, AUTH=3, REMOTE=4`.
- Small, descriptive commits, with `Co-Authored-By` when Claude contributed.
