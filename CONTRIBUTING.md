# Contributing to AutoUpload

Thanks for helping. AutoUpload uploads videos to real YouTube channels, so changes are kept small,
tested, and safe by default.

## Set up

Requires Python 3.10 or newer. On Windows:

```powershell
py -m venv .venv
.\.venv\Scripts\pip install -e .[test]
```

Run the tests:

```powershell
py -3 -m pytest -q
```

## Rules

- **Tests never touch the network.** Use `FakeClient` from `tests/conftest.py` instead of the real YouTube API.
- **Every remote call goes through `YouTubeClient`.** Do not call Google APIs from anywhere else.
- **Edit the website in `docs/_src/{en,es}/`, not the generated HTML.** Then regenerate it:

  ```powershell
  py -3 tools/build_site.py
  ```

- **Code, commit messages and identifiers are in English.** The README and the website are in English and Spanish.
- **Keep commits small and descriptive**, for example `Add retry to thumbnail upload`.
- **Never commit secrets or personal data:** never add `config/client_secret.json`, OAuth tokens, or anything under `channels/`.
  Check `git status` before each commit.

## Opening a pull request

1. Fork the repository and create a branch from `main`.
2. Make your change with tests where the behaviour changes.
3. Run `py -3 -m pytest -q` and make sure it passes.
4. Update the README or the website in `docs/_src/` if the user-facing behaviour changes.
5. Push the branch and open a pull request against `main`. Fill in the checklist in the pull request template.

For bugs and feature ideas, open an issue using one of the templates. For security problems, follow
[SECURITY.md](SECURITY.md) and do not open a public issue.
