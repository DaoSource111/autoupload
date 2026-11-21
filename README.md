<p align="center"><img src="assets/brand/autoupload-logo-rounded-512.png" width="112" alt="AutoUpload logo"></p>

<h1 align="center">AutoUpload</h1>

<p align="center">Upload and schedule YouTube videos from your own computer. No server, no account, your own quota.</p>

<p align="center">
<a href="https://github.com/DaoSource111/autoupload/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/DaoSource111/autoupload"></a>
<img alt="Windows, macOS, Linux" src="https://img.shields.io/badge/Windows%20%C2%B7%20macOS%20%C2%B7%20Linux-native-1f3bff">
<img alt="Written in Rust" src="https://img.shields.io/badge/rust-1.80%2B-orange">
<img alt="YouTube Data API v3" src="https://img.shields.io/badge/YouTube%20Data%20API-v3-5dfbd3">
<a href="LICENSE"><img alt="MIT license" src="https://img.shields.io/badge/license-MIT-lightgrey"></a>
</p>

**English** · [Español](README.es.md)

AutoUpload is a free desktop tool by **Ellipsio Studio** that uploads and schedules videos on the YouTube
channels you connect, using the official YouTube Data API v3. It runs entirely on your computer:
there is no AutoUpload server and no external database. Your data goes only to Google.

- Website, installation guide, privacy policy and terms: https://daosource111.github.io/autoupload/
- Support: ellipsio.studio@gmail.com

## What it does

You place each video in your channel's queue folder (`channels/<channel>/queue/`) with a small YAML file
holding its title and details. AutoUpload:

1. uploads it as a **private** video;
2. gives it the **next free publishing slot** of that channel (or the fixed date you set), so YouTube makes it public at that time;
3. sets its thumbnail, captions and playlist;
4. moves the file to `uploaded/` with a report, and never uploads the same file twice.

It never uploads without `--yes`, never publishes immediately without `--allow-public`, and refuses to
upload if the connected token belongs to a different channel.

## Permissions

AutoUpload requests exactly three Google OAuth scopes: `youtube.upload`, `youtube.force-ssl` and
`youtube.readonly`. What each one is used for is explained on the website.

## Download

Get the file for your system from the [latest release](https://github.com/DaoSource111/autoupload/releases/latest).
Each one holds the desktop app (`autoupload-gui`) and the command line (`autoupload`), both native, with nothing else
to install:

| System | File | First start |
|---|---|---|
| Windows 10/11 (x64) | `AutoUpload-<version>-windows-x64.zip` | unzip and open `autoupload-gui.exe`; if SmartScreen warns, choose *More info → Run anyway* |
| macOS 11+ (Apple silicon) | `AutoUpload-<version>-macos-arm64.zip` | move `AutoUpload.app` to Applications, then right-click it → *Open* the first time |
| macOS 11+ (Intel) | `AutoUpload-<version>-macos-x64.zip` | same as above |
| Linux (x64, glibc 2.35+) | `AutoUpload-<version>-linux-x64.tar.gz` | extract and run `./install.sh` (adds it to your applications menu) or `./autoupload-gui` |

The builds are not code-signed yet, hence the warnings. Verify a download with `SHA256SUMS` or
`gh attestation verify <file> --repo DaoSource111/autoupload`.

## Before the first upload

Each user creates their own OAuth client of type **Desktop app** in Google Cloud, enables the YouTube Data API v3
and saves the client file as `config/client_secret.json` inside the data folder. The step-by-step guide is at
https://daosource111.github.io/autoupload/install.html. The desktop app shows the data folder in its sidebar and
its **Diagnostics** panel tells you exactly what is missing; `autoupload doctor` does the same in a terminal.

## Desktop app

Open **AutoUpload**, create a channel, press **Connect** to sign in with Google, drop videos with their YAML file in
the channel's queue folder and press **Preview dates** to see the plan. **Upload** asks for confirmation before
anything leaves your computer. The **Calendar** tab shows what is already scheduled on YouTube.

## Command line

```powershell
autoupload doctor                         # checks the setup and shows the data folder
autoupload channel new mychannel
autoupload auth mychannel login
autoupload schedule mychannel             # preview: uploads nothing
autoupload schedule mychannel --yes       # upload
autoupload calendar mychannel             # what is scheduled
autoupload data-folder D:\Videos\AutoUpload   # keep channels and config somewhere else
```

Exit codes: `0` OK, `2` invalid input or setup, `3` authentication, `4` YouTube error.

## Channel settings (`channels/<channel>/channel.yaml`)

| Key | Default | Meaning |
|---|---|---|
| `profile` | channel name | name of the OAuth token |
| `client_secret` | `../../config/client_secret.json` | Desktop OAuth client file |
| `timezone` | `"+00:00"` | zone of `hours`: a name that follows daylight saving (`Europe/Madrid`, `America/Lima`) or a fixed UTC offset (`+00:00`, `-05:00`) |
| `hours` | `["08:00"]` | publishing times for each day |
| `days_between` | `1` | 1 = every day, 7 = once a week |
| `min_lead_hours` | `3` | never schedule closer than this to now |
| `max_per_run` | `5` | uploads per run |
| `language` | `en` | language of title, description and audio |
| `category` | `"27"` | YouTube category ID (27 Education, 28 Science & Technology, 24 Entertainment) |
| `made_for_kids` | `false` | "made for kids" declaration |
| `contains_ai` | `null` | altered or synthetic content declaration |
| `notify_subscribers` | `false` | notify subscribers on publication |
| `playlist` | `null` | playlist ID for every upload |
| `base_tags` | `[]` | tags added to every video |
| `description_footer` | `""` | text appended to every description |

Unknown keys are rejected, so a typo is reported instead of silently ignored.

## Video file (`<video>.yaml` next to each video)

Only `title` is required. Common keys: `description`, `tags`, `thumbnail`, `publish_at` (ISO 8601 with
offset, e.g. `2026-11-21T08:00:00+00:00`), `playlist_id`, `privacy`, `category_id`, `default_language`,
`made_for_kids`, `contains_synthetic_media`, `notify_subscribers`, `captions`, `localizations`.

## Daily run (optional)

`install-task.ps1 -Time 10:00` registers a Windows scheduled task named `AutoUpload` that runs
`upload-all.ps1` (`autoupload schedule --all --yes`) every day and writes a log to `logs\`. On macOS and Linux,
add the same command to `crontab -e`, e.g. `0 10 * * * ~/.local/bin/autoupload schedule --all --yes >> ~/autoupload.log 2>&1`.

## Security

- `config/client_secret.json`, OAuth tokens and channel data must never be committed. `.gitignore` excludes them.
- OAuth tokens are stored in `%APPDATA%\AutoUpload` on Windows and `~/.config/autoupload` on macOS and Linux
  (override with `AUTOUPLOAD_CONFIG_DIR`); on macOS and Linux the files are readable only by your user.
- `channels/` and `config/` live in the data folder, chosen in this order: `AUTOUPLOAD_HOME`; the folder set with
  `autoupload data-folder` or the app's **Change…** button; a `channels/` or `config/` folder next to the program
  (portable copy); otherwise `Documents\AutoUpload` (`~/AutoUpload` on macOS and Linux).
- Version 2 reads the tokens, channel files and upload history of version 1.x unchanged.

## Build from source

Requires Rust 1.80+ (and on Linux `libfontconfig1-dev libxkbcommon-dev`):

```powershell
cargo test --workspace
cargo build --release          # target/release/autoupload and autoupload-gui
```

## Repository layout

| Path | Contents |
|---|---|
| `crates/core/` | the engine: channels, queue, OAuth, YouTube client, resumable upload, state (`cargo test`) |
| `crates/cli/` | the `autoupload` command line |
| `crates/gui/` | the desktop app (Slint) |
| `packaging/` | Linux menu entry and installer, macOS bundle metadata |
| `src/autoupload/`, `tests/` | the 1.x Python version, kept until 2.0 has been proven on real uploads |
| `docs/` | the public website, served by GitHub Pages; edit `docs/_src/` and run `python tools/build_site.py` |
| `install-task.ps1`, `upload-all.ps1` | optional daily run as a Windows scheduled task |

## License

MIT © 2026 Ellipsio Studio. AutoUpload is not affiliated with, endorsed or sponsored by Google LLC or YouTube.
