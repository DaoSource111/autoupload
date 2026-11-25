//! The daily unattended run, registered with each system's own scheduler: Task Scheduler on Windows, launchd on
//! macOS, a systemd user timer on Linux. It replaces the 1.x `install-task.ps1` and `upload-all.ps1` scripts.
//!
//! The scheduled command is `autoupload task run`: it uploads the queue of every channel (never as public), writes
//! a log to `<data folder>/logs/` (kept 60 days) and records each channel's result in the activity log.

use chrono::{Local, NaiveTime};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::activity::{Actor, TASK};
use crate::channel::{list_channels, load_channel};
use crate::client::api_for;
use crate::paths::{data_root, RootSource, Settings};
use crate::queue::queue_items;
use crate::runner::{process_queue, Event, RunOptions};

pub const LOG_DAYS: u64 = 60;
#[cfg(target_os = "macos")]
const LABEL: &str = "studio.ellipsio.autoupload";

pub fn parse_time(s: &str) -> Result<NaiveTime, String> {
    NaiveTime::parse_from_str(s.trim(), "%H:%M")
        .map_err(|_| format!("'{s}' is not a time like 10:00"))
}

fn exe() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| format!("cannot find this program's path: {e}"))
}

/// The task runs without this shell's environment, so a data folder given only by AUTOUPLOAD_HOME is saved
/// to settings.json, where the task finds it.
fn pin_data_folder() -> Result<Option<PathBuf>, String> {
    let (home, source) = data_root();
    if source == RootSource::Env {
        let mut s = Settings::load();
        s.data_folder = Some(home.clone());
        s.save()
            .map_err(|e| format!("could not save the data folder: {e}"))?;
        return Ok(Some(home));
    }
    Ok(None)
}

fn run_cmd(cmd: &mut Command) -> Result<String, String> {
    let out = cmd
        .output()
        .map_err(|e| format!("could not start {:?}: {e}", cmd.get_program()))?;
    let text =
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    if out.status.success() {
        Ok(text)
    } else {
        Err(text.trim().to_string())
    }
}

/// Registers (or replaces) the daily task at `time`, local time. Returns what was done, for the user.
pub fn install(time: NaiveTime) -> Result<String, String> {
    let exe = exe()?;
    let pinned = pin_data_folder()?;
    let mut msg = install_os(&exe, time)?;
    if let Some(h) = pinned {
        msg.push_str(&format!(
            "\nThe data folder {} was saved to settings so the task finds it.",
            h.display()
        ));
    }
    Ok(msg)
}

#[cfg(windows)]
fn install_os(exe: &Path, time: NaiveTime) -> Result<String, String> {
    let tr = format!("\"{}\" task run", exe.display());
    run_cmd(Command::new("schtasks").args([
        "/Create",
        "/F",
        "/SC",
        "DAILY",
        "/TN",
        "AutoUpload",
        "/ST",
        &time.format("%H:%M").to_string(),
        "/TR",
        &tr,
    ]))?;
    Ok(format!(
        "Task Scheduler: 'AutoUpload' runs every day at {}. Remove it with: autoupload task remove",
        time.format("%H:%M")
    ))
}

#[cfg(target_os = "macos")]
fn agent_plist() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").ok_or("HOME is not set")?;
    Ok(PathBuf::from(home)
        .join("Library/LaunchAgents")
        .join(format!("{LABEL}.plist")))
}

#[cfg(target_os = "macos")]
fn install_os(exe: &Path, time: NaiveTime) -> Result<String, String> {
    use chrono::Timelike;
    let plist = agent_plist()?;
    let body = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LABEL}</string>
  <key>ProgramArguments</key><array><string>{}</string><string>task</string><string>run</string></array>
  <key>StartCalendarInterval</key><dict><key>Hour</key><integer>{}</integer><key>Minute</key><integer>{}</integer></dict>
</dict>
</plist>
"#,
        exe.display(),
        time.hour(),
        time.minute()
    );
    std::fs::create_dir_all(plist.parent().unwrap_or(Path::new("."))).map_err(|e| e.to_string())?;
    let _ = run_cmd(Command::new("launchctl").arg("unload").arg(&plist));
    std::fs::write(&plist, body).map_err(|e| e.to_string())?;
    run_cmd(Command::new("launchctl").args(["load", "-w"]).arg(&plist))?;
    Ok(format!(
        "launchd: {} runs every day at {}.",
        plist.display(),
        time.format("%H:%M")
    ))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn systemd_dir() -> Result<PathBuf, String> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .ok_or("HOME is not set")?;
    Ok(base.join("systemd/user"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn install_os(exe: &Path, time: NaiveTime) -> Result<String, String> {
    let dir = systemd_dir()?;
    let cron = format!(
        "{} {} * * * \"{}\" task run",
        time.format("%M"),
        time.format("%H"),
        exe.display()
    );
    if Command::new("systemctl").arg("--version").output().is_err() {
        return Err(format!(
            "systemd is not available. Add this line with `crontab -e` instead:\n{cron}"
        ));
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(
        dir.join("autoupload.service"),
        format!(
            "[Unit]\nDescription=AutoUpload daily run\n\n[Service]\nType=oneshot\nExecStart=\"{}\" task run\n",
            exe.display()
        ),
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(
        dir.join("autoupload.timer"),
        format!(
            "[Unit]\nDescription=AutoUpload every day at {t}\n\n[Timer]\nOnCalendar=*-*-* {t}:00\nPersistent=true\n\n[Install]\nWantedBy=timers.target\n",
            t = time.format("%H:%M")
        ),
    )
    .map_err(|e| e.to_string())?;
    run_cmd(Command::new("systemctl").args(["--user", "daemon-reload"]))?;
    run_cmd(Command::new("systemctl").args(["--user", "enable", "--now", "autoupload.timer"]))?;
    Ok(format!(
        "systemd: autoupload.timer runs every day at {} (catching up after the computer was off).",
        time.format("%H:%M")
    ))
}

pub fn remove() -> Result<String, String> {
    #[cfg(windows)]
    {
        run_cmd(Command::new("schtasks").args(["/Delete", "/F", "/TN", "AutoUpload"]))?;
        Ok("Task 'AutoUpload' removed.".into())
    }
    #[cfg(target_os = "macos")]
    {
        let plist = agent_plist()?;
        let _ = run_cmd(Command::new("launchctl").arg("unload").arg(&plist));
        std::fs::remove_file(&plist).map_err(|e| e.to_string())?;
        Ok(format!("{} removed.", plist.display()))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = run_cmd(Command::new("systemctl").args([
            "--user",
            "disable",
            "--now",
            "autoupload.timer",
        ]));
        let dir = systemd_dir()?;
        let _ = std::fs::remove_file(dir.join("autoupload.timer"));
        let _ = std::fs::remove_file(dir.join("autoupload.service"));
        let _ = run_cmd(Command::new("systemctl").args(["--user", "daemon-reload"]));
        Ok("autoupload.timer removed.".into())
    }
}

pub fn status() -> String {
    #[cfg(windows)]
    {
        match run_cmd(Command::new("schtasks").args(["/Query", "/TN", "AutoUpload", "/FO", "LIST"]))
        {
            Ok(t) => t.trim().to_string(),
            Err(_) => "No daily task. Create one with: autoupload task install --time 10:00".into(),
        }
    }
    #[cfg(target_os = "macos")]
    {
        match agent_plist() {
            Ok(p) if p.is_file() => format!("Daily task installed: {}", p.display()),
            _ => "No daily task. Create one with: autoupload task install --time 10:00".into(),
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        match run_cmd(Command::new("systemctl").args([
            "--user",
            "list-timers",
            "autoupload.timer",
            "--all",
        ])) {
            Ok(t) if t.contains("autoupload.timer") => t.trim().to_string(),
            _ => "No daily task. Create one with: autoupload task install --time 10:00".into(),
        }
    }
}

/// Deletes logs older than [`LOG_DAYS`].
fn rotate_logs(dir: &Path) {
    let limit = std::time::Duration::from_secs(LOG_DAYS * 86_400);
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .map(|age| age > limit)
            .unwrap_or(false);
        if old && e.path().extension().is_some_and(|x| x == "log") {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// The scheduled run: uploads every channel's queue. Returns the log path and whether everything went well.
pub fn run_daily(home: &Path) -> (PathBuf, bool) {
    let dir = home.join("logs");
    let _ = std::fs::create_dir_all(&dir);
    rotate_logs(&dir);
    let path = dir.join(format!("{}.log", Local::now().format("%Y-%m-%d_%H%M")));
    let mut log = std::fs::File::create(&path).ok();
    let mut line = |s: &str| {
        println!("{s}");
        if let Some(f) = log.as_mut() {
            let _ = writeln!(f, "{} {s}", Local::now().format("%H:%M:%S"));
        }
    };
    let actor = Actor::new(TASK);
    let mut all_ok = true;
    line(&format!(
        "AutoUpload {} daily run, data folder {}",
        crate::VERSION,
        home.display()
    ));
    for name in list_channels(home) {
        let ch = match load_channel(&name, home) {
            Ok(c) => c,
            Err(e) => {
                line(&format!("[{name}] invalid channel.yaml: {e}"));
                all_ok = false;
                continue;
            }
        };
        if queue_items(&ch).is_empty() {
            line(&format!("[{name}] queue empty"));
            continue;
        }
        let api = match api_for(&ch) {
            Ok(a) => a,
            Err(e) => {
                line(&format!("[{name}] not signed in: {e}"));
                actor.record(
                    home,
                    Some(&name),
                    "run",
                    format!("Daily run stopped: not signed in ({e})"),
                    false,
                );
                all_ok = false;
                continue;
            }
        };
        let opts = RunOptions {
            execute: true,
            ..Default::default()
        };
        let mut done = 0;
        let mut errors = 0;
        let res = process_queue(&ch, &*api, &opts, &mut |e| match e {
            Event::Done {
                file, url, when, ..
            } => {
                done += 1;
                line(&format!("[{name}] ✓ {file} → {url} ({when})"));
            }
            Event::Error(s) => {
                errors += 1;
                line(&format!("[{name}] ✗ {s}"));
            }
            Event::Warning(s) | Event::Info(s) => line(&format!("[{name}] {s}")),
            Event::Planned { .. } | Event::Uploading { .. } | Event::Progress { .. } => {}
        });
        match res {
            Ok(_) => actor.record(
                home,
                Some(&name),
                "run",
                format!(
                    "Daily run: {done} video(s) uploaded{}",
                    if errors > 0 {
                        format!(", {errors} error(s)")
                    } else {
                        String::new()
                    }
                ),
                errors == 0,
            ),
            Err(stop) => {
                line(&format!("[{name}] stopped: {stop}"));
                actor.record(
                    home,
                    Some(&name),
                    "run",
                    format!("Daily run stopped: {stop}"),
                    false,
                );
            }
        }
        all_ok &= errors == 0;
    }
    (path, all_ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times() {
        assert_eq!(
            parse_time("9:05").unwrap().format("%H:%M").to_string(),
            "09:05"
        );
        assert!(parse_time("25:00").is_err());
    }

    #[test]
    fn old_logs_are_deleted() {
        let d = tempfile::tempdir().unwrap();
        let old = d.path().join("old.log");
        let new = d.path().join("new.log");
        std::fs::write(&old, "x").unwrap();
        std::fs::write(&new, "x").unwrap();
        let past =
            std::time::SystemTime::now() - std::time::Duration::from_secs((LOG_DAYS + 1) * 86_400);
        std::fs::File::options()
            .write(true)
            .open(&old)
            .unwrap()
            .set_modified(past)
            .unwrap();
        rotate_logs(d.path());
        assert!(!old.exists() && new.exists());
    }
}
