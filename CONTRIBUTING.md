# Contributing to AutoUpload

Thanks for helping. AutoUpload uploads videos to real YouTube channels, so changes are kept small,
tested, and safe by default.

## Set up

Requires [Rust](https://rustup.rs) 1.80 or newer. On Linux, also install the desktop app's libraries:
`sudo apt-get install libfontconfig1-dev libxkbcommon-dev`.

```powershell
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo run -p autoupload-gui          # the desktop app
cargo run -p autoupload -- doctor    # the command line
```

The 1.x Python version in `src/autoupload/` is kept until 2.0 has proven itself; its tests run with
`py -3 -m pytest -q` after `pip install -e .[test]`. New work goes into the Rust crates.

## Rules

- **Tests never touch the network.** Use `FakeApi` (`crates/core/src/publisher.rs`) instead of the real YouTube API.
- **Every remote call goes through the `Api` trait** (`crates/core/src/youtube.rs`). Do not call Google from anywhere else.
- **Stay compatible with 1.x data:** the token JSON, `channel.yaml` keys and the `state.sqlite` schema change only
  with a migration.
- **Edit the website in `docs/_src/{en,es}/`, not the generated HTML.** Then regenerate it:

  ```powershell
  py -3 tools/build_site.py
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
