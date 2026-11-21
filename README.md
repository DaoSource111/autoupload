<p align="center"><img src="assets/brand/autoupload-logo-rounded-512.png" width="112" alt="AutoUpload logo"></p>

<h1 align="center">AutoUpload</h1>

<p align="center">Upload and schedule YouTube videos from your own computer. No server, no account, your own quota.</p>

<p align="center">
<a href="https://github.com/DaoSource111/autoupload/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/DaoSource111/autoupload"></a>
<img alt="Python 3.10+" src="https://img.shields.io/badge/python-3.10%2B-1f3bff">
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

## Quick start (Windows, Python 3.10+)

```powershell
py -m venv .venv
.\.venv\Scripts\pip install -e .
.\.venv\Scripts\autoupload.exe doctor                    # checks the setup and shows the data folder
.\.venv\Scripts\autoupload.exe channel new mychannel
.\.venv\Scripts\autoupload.exe auth mychannel login
.\.venv\Scripts\autoupload.exe schedule mychannel           # preview: uploads nothing
.\.venv\Scripts\autoupload.exe schedule mychannel --yes     # upload
```

Before `auth … login`, create your own OAuth client of type **Desktop app** in Google Cloud, enable the
YouTube Data API v3 and save the client file as `config\client_secret.json` inside the data folder
(`Documents\AutoUpload` for an installed copy; the repository folder when you run from source with `pip install -e .`;
`autoupload doctor` prints it). The step-by-step guide, for
doing it by hand or with an AI agent, is at https://daosource111.github.io/autoupload/install.html

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
`upload-all.ps1` (`autoupload schedule --all --yes`) every day and writes a log to `logs\`.

## Security

- `config/client_secret.json`, OAuth tokens and channel data must never be committed. `.gitignore` excludes them.
- OAuth tokens are stored in `%APPDATA%\AutoUpload` on your own computer (override with `AUTOUPLOAD_CONFIG_DIR`).
  `channels/` and `config/` live in the data folder: `AUTOUPLOAD_HOME` if set, else the repository when running
  from source, else `Documents\AutoUpload` (`~/AutoUpload` on macOS/Linux). Version 1.1.0 kept them inside the
  venv (`.venv\Lib\`) when installed with `pip install .`; `autoupload doctor` detects that and says where to move them.

## Repository layout

| Path | Contents |
|---|---|
| `src/autoupload/` | the application |
| `tests/` | tests (`pip install .[test]` then `pytest`) |
| `docs/` | the public website, served by GitHub Pages; edit `docs/_src/` and run `python tools/build_site.py` |
| `install-task.ps1`, `upload-all.ps1` | optional daily run as a Windows scheduled task |

## License

MIT © 2026 Ellipsio Studio. AutoUpload is not affiliated with, endorsed or sponsored by Google LLC or YouTube.
