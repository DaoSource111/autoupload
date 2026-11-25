# AutoUpload for AI agents

This file is for AI agents. The first part explains how to **operate** AutoUpload for a person (queue their videos,
write the details, preview, upload). The second part explains how to **work on this repository**.

## Part 1 · Operating AutoUpload

AutoUpload uploads videos to the person's own YouTube channels as private videos with a scheduled publish time.
Everything runs on their computer; it talks only to Google, with their own OAuth client.

### Connect

Prefer MCP. The person registers the server once:

- Claude Code: `claude mcp add autoupload -- autoupload mcp`
- Codex CLI, in `~/.codex/config.toml`: `[mcp_servers.autoupload]` with `command = "autoupload"` and `args = ["mcp"]`
- Any MCP client: stdio server, command `autoupload`, argument `mcp`

`autoupload agents setup` prints these lines with the program's real path. To practise without touching a real
channel, use `autoupload --demo mcp`: sample channels and an offline YouTube.

Without MCP, use the terminal: set `AUTOUPLOAD_AGENT=<your name>` in the environment of every command and add
`--json`. Your actions are then logged under your name and the same permissions apply.

### Tools

| MCP tool | Terminal equivalent | Notes |
|---|---|---|
| `status` | `autoupload channels --json` | Start here: channels, whether each is signed in, queue sizes, `agents_may_upload`. |
| `list_queue` | `autoupload queue <channel> --json` | Each item has `problem` (why it cannot be uploaded) or `null`. |
| `add_video` | `autoupload add <channel> <video> --title … --json` | Copies by default (`move: true` / `--move` to move). `title` is required (≤ 100 characters). |
| `write_sheet` | `autoupload sheet <channel> <file> --title … --json` | Replaces the details file of a queued video. |
| `plan` | `autoupload schedule <channel> --json` | The date each video would get. Uploads nothing. |
| `upload` | `autoupload schedule <channel> --yes --json` | Refused unless the person allowed agents to upload. |
| `calendar` | `autoupload calendar <channel> --json` | Published and scheduled videos. |
| `doctor` | `autoupload doctor --json` | Setup problems, each with its fix. |
| `activity` | `autoupload activity --json` | What the person, the daily task and agents did. |

### A good session

1. `status`. If a channel is not signed in, ask the person to press **Connect** in the app (or run
   `autoupload auth <channel> login`); you cannot sign in for them.
2. For each new video: look at what you can (file name, any notes or transcript the person gave you) and call
   `add_video` with a specific title, a useful description and relevant tags. Do not invent facts you cannot see:
   ask. Set `contains_synthetic_media: true` when the video has realistic AI-generated or altered voice or imagery.
3. `list_queue` and fix every `problem` (usually with `write_sheet`).
4. `plan`, and show the person the dates.
5. If `status` says `agents_may_upload` is true and the person asked you to upload, call `upload`. Otherwise tell them
   to press **Upload** in the app, or to allow agents to upload (app: AI agents → *Agents may upload*).

### Rules you cannot change

- Uploads are private with a scheduled date; agents can never publish as public at once.
- Uploading needs the person's permission; changing that permission is refused to agents.
- Never read, print or send `config/client_secret.json` or token files.
- A video copied into the queue waits about two minutes (it might still be copying) before a run takes it.
- YouTube allows about 100 uploads per day per Google Cloud project; a spent quota stops the run until midnight
  Pacific Time. Do not retry in a loop.

## Part 2 · Working on this repository

- Read `CLAUDE.md` (commands, folder map, systems, conventions) and `docs/progress.md` first.
- Everything is Rust: `crates/core` (engine), `crates/cli` (CLI and MCP server), `crates/gui` (Slint app),
  `tools/site` (website generator).
- Before a change is done: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`. Tests never use the network: `FakeApi` and `DemoApi` stand in for YouTube.
- Never run `schedule --yes`, `auth … login` or the app's Upload/Connect against a real channel unless the person
  asks. Use `--demo` to try things.
- Documentation is written in English first; `README.es.md` and `docs/_src/es/` are translations, updated after.
  The website is generated: edit `docs/_src/`, then `cargo run -p autoupload-site`.
- Keep files from 1.x readable: the token JSON, `channel.yaml` keys and the `state.sqlite` schema change only with a
  migration.
