//! `autoupload doctor`: offline checks of the installation, with the fix for each problem.
//! Nothing here touches the network or prints secrets: the client file and tokens are only checked for shape.

use serde::Serialize;
use std::path::Path;

use crate::auth::{read_client, read_token, ClientKind};
use crate::channel::{list_channels, load_channel};
use crate::paths::{data_root, RootSource};
use crate::validate::find_ffprobe;
use crate::GUIDE_URL;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Ok,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub level: Level,
    pub message: String,
    pub fix: String,
}

fn c(level: Level, message: impl Into<String>, fix: impl Into<String>) -> Check {
    Check {
        level,
        message: message.into(),
        fix: fix.into(),
    }
}

pub fn run_at(home: &Path, source: RootSource) -> Vec<Check> {
    let mut out = vec![c(Level::Ok, format!("AutoUpload {}", crate::VERSION), "")];
    out.push(c(
        Level::Ok,
        format!("data folder: {} ({})", home.display(), source.describe()),
        "",
    ));
    match find_ffprobe() {
        Some(_) => out.push(c(Level::Ok, "ffprobe found", "")),
        None => out.push(c(
            Level::Warning,
            "ffprobe not found",
            "optional: install FFmpeg so video and caption durations are checked",
        )),
    }
    let names = list_channels(home);
    if names.is_empty() {
        out.push(c(
            Level::Warning,
            "no channels configured",
            "create one: autoupload channel new <name>",
        ));
    }
    let mut any_token = false;
    for n in names {
        let ch = match load_channel(&n, home) {
            Ok(ch) => ch,
            Err(e) => {
                out.push(c(Level::Error, format!("[{n}] invalid channel.yaml"), e));
                continue;
            }
        };
        match read_client(&ch.client_secret) {
            ClientKind::Desktop(_) => out.push(c(Level::Ok, format!("[{n}] OAuth client (Desktop app) found"), "")),
            ClientKind::Missing => out.push(c(
                Level::Error,
                format!("[{n}] OAuth client file not found: {}", ch.client_secret.display()),
                format!("create a Desktop app OAuth client in your Google Cloud project and save it there; guide: {GUIDE_URL}#client"),
            )),
            ClientKind::Web => out.push(c(Level::Error, format!("[{n}] the client file is a Web application client"), "create a client of type Desktop app instead")),
            ClientKind::Invalid => out.push(c(Level::Error, format!("[{n}] the client file is not an OAuth client file"), "download it again from Google Cloud > Clients")),
        }
        match read_token(&ch.profile) {
            None => out.push(c(
                Level::Warning,
                format!("[{n}] not connected"),
                format!("autoupload auth {n} login"),
            )),
            Some(t) if t.refresh_token.is_none() => out.push(c(
                Level::Error,
                format!("[{n}] saved sign-in cannot be renewed"),
                format!("autoupload auth {n} login"),
            )),
            Some(_) => {
                any_token = true;
                let linked = ch
                    .expected_channel_id()
                    .map(|id| format!(" to {id}"))
                    .unwrap_or_default();
                out.push(c(Level::Ok, format!("[{n}] connected{linked}"), ""));
            }
        }
        let queued = std::fs::read_dir(ch.queue())
            .map(|r| r.count())
            .unwrap_or(0);
        out.push(c(
            Level::Ok,
            format!("[{n}] {queued} file(s) in queue, {}", ch.schedule_text()),
            "",
        ));
    }
    if any_token {
        out.push(c(
            Level::Ok,
            "note: if your Google app is in Testing mode, the sign-in expires after 7 days",
            format!("use Personal mode or publish the app to avoid it; guide: {GUIDE_URL}"),
        ));
    }
    out
}

pub fn run() -> Vec<Check> {
    let (home, source) = data_root();
    run_at(&home, source)
}

pub fn render(checks: &[Check]) -> String {
    let mut lines = Vec::new();
    for ch in checks {
        let mark = match ch.level {
            Level::Ok => "✓",
            Level::Warning => "!",
            Level::Error => "✗",
        };
        lines.push(format!("{mark} {}", ch.message));
        if !ch.fix.is_empty() && (ch.level != Level::Ok || ch.message.starts_with("note:")) {
            lines.push(format!("    → {}", ch.fix));
        }
    }
    let errors = checks.iter().filter(|c| c.level == Level::Error).count();
    let warnings = checks.iter().filter(|c| c.level == Level::Warning).count();
    lines.push(format!("{errors} error(s), {warnings} warning(s)"));
    lines.join("\n")
}

pub fn has_errors(checks: &[Check]) -> bool {
    checks.iter().any(|c| c.level == Level::Error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::create_channel;

    #[test]
    fn missing_client_is_an_error_and_secrets_are_never_printed() {
        let d = tempfile::tempdir().unwrap();
        create_channel("a", d.path()).unwrap();
        let checks = run_at(d.path(), RootSource::Env);
        assert!(has_errors(&checks));
        std::fs::create_dir_all(d.path().join("config")).unwrap();
        std::fs::write(
            d.path().join("config/client_secret.json"),
            r#"{"installed": {"client_id": "x", "client_secret": "S3CRET"}}"#,
        )
        .unwrap();
        let checks = run_at(d.path(), RootSource::Env);
        let text = render(&checks);
        assert!(text.contains("Desktop app") && !text.contains("S3CRET"));
    }
}
