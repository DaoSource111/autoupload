# Contributing to AutoUpload

Thanks for helping. AutoUpload uploads videos to real YouTube channels, so changes are kept small,
tested, and safe by default.

## Set up

Requires [Rust](https://rustup.rs) 1.95 or newer. On Linux, also install the desktop app's libraries:
`sudo apt-get install libfontconfig1-dev libxkbcommon-dev`.

```powershell
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo run -p autoupload-gui          # the desktop app
cargo run -p autoupload -- doctor    # the command line
```

Everything is Rust. `cargo run -p autoupload-gui -- --demo` and `cargo run -p autoupload -- --demo <command>` work on
sample data with an offline YouTube, so you can try any change without a Google account.

## Rules

- **Tests never touch the network.** Use `FakeApi` (`crates/core/src/publisher.rs`) or `DemoApi` (`crates/core/src/demo.rs`)
  instead of the real YouTube API.
- **Every remote call goes through the `Api` trait** (`crates/core/src/youtube.rs`, chosen by `client::api_for`).
  Do not call Google from anywhere else.
- **Record user-visible actions** in the activity log (`activity::Actor::record`), and keep agents' limits in
  `crates/core/src/ops.rs`: they may prepare and preview, upload only with permission, never publish as public.
- **Screenshots use demo mode only**: no real channel, account or local user path may appear in them.
- **Stay compatible with 1.x data:** the token JSON, `channel.yaml` keys and the `state.sqlite` schema change only
  with a migration.
- **Edit the website in `docs/_src/{en,es}/`, not the generated HTML.** Then regenerate it:

  ```powershell
  cargo run -p autoupload-site
  ```

- **Code, commit messages and identifiers are in English.** The README and the website are in English and Spanish;
  English is written first.
- **Keep commits small and descriptive**, for example `Add retry to thumbnail upload`.
- **Never commit secrets or personal data:** never add `config/client_secret.json`, OAuth tokens, or anything under `channels/`.
  Check `git status` before each commit.

## Opening a pull request

1. Fork the repository and create a branch from `main`.
2. Make your change with tests where the behaviour changes.
3. Run `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` and make sure both pass.
4. Update the README or the website in `docs/_src/` if the user-facing behaviour changes.
5. Push the branch and open a pull request against `main`. Fill in the checklist in the pull request template.

For bugs and feature ideas, open an issue using one of the templates. For security problems, follow
[SECURITY.md](SECURITY.md) and do not open a public issue.
