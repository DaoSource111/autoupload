# AutoUpload

AutoUpload is a free desktop tool by **Ellipsio Studio** that uploads and schedules videos on the YouTube
channels you connect, using the official YouTube Data API v3. It runs entirely on your computer:
there is no AutoUpload server and no external database. Your data goes only to Google.

- Website, installation guide, privacy policy and terms: https://daosource111.github.io/autoupload/
- Support: ellipsio.studio@gmail.com

## What it does

You place each video in your channel's queue folder (`canales/<channel>/queue/`) with a small YAML file
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
.\.venv\Scripts\pip install .
.\.venv\Scripts\subidor.exe channel new mychannel
.\.venv\Scripts\subidor.exe auth mychannel login
.\.venv\Scripts\subidor.exe schedule mychannel           # preview: uploads nothing
.\.venv\Scripts\subidor.exe schedule mychannel --yes     # upload
```

Before `auth … login`, create your own OAuth client of type **Desktop app** in Google Cloud and save it as
`config/client_secret.json`. The full step-by-step guide is at
https://daosource111.github.io/autoupload/install.html

Every command also accepts its original Spanish name (`canal nuevo`, `programar`, `--todos`...), and existing
channels with Spanish folder names (`cola/`, `subidos/`) and configuration keys keep working.

## Security

- `config/client_secret.json`, OAuth tokens and channel data must never be committed. `.gitignore` excludes them.
- The OAuth token is stored in `%APPDATA%\subidor-youtube` on your own computer.

## Repository layout

| Path | Contents |
|---|---|
| `src/subidor/` | the application |
| `tests/` | tests (`pip install .[test]` then `pytest`) |
| `docs/` | the public website, served by GitHub Pages; edit `docs/_src/` and run `python tools/build_site.py` |
| `subir-todo.ps1`, `instalar-tarea.ps1` | optional daily run as a Windows scheduled task |

## License

MIT © 2026 Ellipsio Studio. AutoUpload is not affiliated with, endorsed or sponsored by Google LLC or YouTube.
