# Security Policy

## Supported versions

| Version | Supported |
|---|---|
| 2.0.x | Yes |
| 1.2.x | Security fixes until 2.0 has replaced it |
| 1.1.x and older | No, please upgrade to 2.0.x |

## Reporting a vulnerability

Please report security problems **privately**. Do not open a public issue or pull request for them.

- Email: ellipsio.studio@gmail.com
- Or use GitHub Security Advisories: the **Security** tab of this repository, then **Report a vulnerability**.

Include the version, your operating system, the steps to reproduce, and the impact you expect. We aim to
reply within 7 days. This is a guideline, not a guarantee; the project is maintained by a small studio.

## What data the app handles

- AutoUpload runs on your computer. There is no AutoUpload server and no external database.
- Your Google OAuth token is stored locally in `%APPDATA%\AutoUpload` (or the folder set in `AUTOUPLOAD_CONFIG_DIR`).
- Your OAuth client file (`config/client_secret.json`) and your channel data stay in your data folder.
- Requests go only from your computer to Google's YouTube Data API v3.
- The app requests three Google scopes: `youtube.upload`, `youtube.force-ssl` and `youtube.readonly`.

## Out of scope

- The security of your Google account, your password, or two-step verification. Protect those with Google's own tools.
- Problems caused by sharing your token, client file, or `channels/` folder with someone else.
- Vulnerabilities in Google's services or the YouTube API itself. Report those to Google.
- Issues that need a machine that is already compromised by malware.
