//! AutoUpload desktop app (Slint). Network work runs on worker threads; the window never blocks.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use autoupload_core::activity::{self, Actor, APP};
use autoupload_core::auth;
use autoupload_core::channel::{create_channel, list_channels, load_channel, Channel};
use autoupload_core::client::api_for;
use autoupload_core::demo;
use autoupload_core::doctor::{self, Level};
use autoupload_core::fsutil::write_atomic;
use autoupload_core::manifest::read_sheet;
use autoupload_core::paths::{data_root, Settings};
use autoupload_core::queue::{is_settling, queue_items, SETTLE};
use autoupload_core::runner::{calendar_rows, process_queue, Event, RunOptions};
use chrono::{Local, Utc};
use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel, Weak};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

slint::include_modules!();

#[derive(Default)]
struct Ctx {
    home: PathBuf,
    names: Vec<String>,
    channel: Option<Channel>,
    /// Modification time of activity.jsonl when the Activity tab was last filled.
    activity_seen: Option<SystemTime>,
}

thread_local! {
    static CTX: RefCell<Ctx> = RefCell::new(Ctx::default());
}

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(v))
}

/// Details of a run go to stderr (visible when started from a terminal); the Activity tab shows the summary.
fn log_line(_ui: &AppWindow, line: impl Into<SharedString>) {
    eprintln!("{}  {}", Local::now().format("%H:%M:%S"), line.into());
}

fn home() -> PathBuf {
    CTX.with(|c| c.borrow().home.clone())
}

/// Records an action taken in the app in the shared activity log, then refreshes the Activity tab.
fn record(
    ui: &AppWindow,
    channel: Option<&str>,
    action: &str,
    summary: impl Into<String>,
    ok: bool,
) {
    Actor::new(APP).record(&home(), channel, action, summary, ok);
    refresh_activity(ui, true);
}

/// Fills the Activity tab from activity.jsonl when it changed (or always with `force`).
fn refresh_activity(ui: &AppWindow, force: bool) {
    let home = home();
    let modified = std::fs::metadata(activity::log_path(&home))
        .and_then(|m| m.modified())
        .ok();
    let unchanged = CTX.with(|c| c.borrow().activity_seen == modified);
    if unchanged && !force {
        return;
    }
    CTX.with(|c| c.borrow_mut().activity_seen = modified);
    let today = Local::now().date_naive();
    let rows: Vec<ActivityRow> = activity::recent(&home, 300)
        .into_iter()
        .rev()
        .map(|e| {
            let local = e.time.with_timezone(&Local);
            let time = if local.date_naive() == today {
                local.format("Today %H:%M").to_string()
            } else {
                local.format("%b %d · %H:%M").to_string()
            };
            ActivityRow {
                time: time.into(),
                who: activity::who(&e).into(),
                channel: e.channel.clone().unwrap_or_default().into(),
                summary: e.summary.clone().into(),
                ok: e.ok,
                agent: e.actor == activity::AGENT,
            }
        })
        .collect();
    ui.set_activity(model(rows));
}

/// The command-line program next to this one, which agents start as their MCP server.
fn cli_path() -> PathBuf {
    let name = if cfg!(windows) {
        "autoupload.exe"
    } else {
        "autoupload"
    };
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(name)))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(name))
}

fn agent_instructions(ui: &AppWindow) {
    // In demo mode the plain name is shown, so screenshots carry no local paths.
    let cli = if demo::active() {
        PathBuf::from("autoupload")
    } else {
        cli_path()
    };
    let cli = cli.display();
    let args = if demo::active() { "--demo mcp" } else { "mcp" };
    let toml_args = if demo::active() {
        r#"["--demo", "mcp"]"#
    } else {
        r#"["mcp"]"#
    };
    ui.set_agent_claude(format!("claude mcp add autoupload -- \"{cli}\" {args}").into());
    // A TOML literal string ('…') keeps Windows backslashes as they are.
    ui.set_agent_codex(
        format!("[mcp_servers.autoupload]\ncommand = '{cli}'\nargs = {toml_args}").into(),
    );
    ui.set_agent_other(format!("command: {cli}\narguments: {args}").into());
}

fn demo_or_real(path: &Path) -> String {
    if demo::active() {
        "the demo folder".into()
    } else {
        path.display().to_string()
    }
}

fn banner(ui: &AppWindow, text: impl Into<SharedString>, tone: i32) {
    ui.set_banner(text.into());
    ui.set_banner_tone(tone);
}

/// Queue rows straight from disk (before a preview gives dates).
fn disk_rows(ch: &Channel) -> (Vec<QueueRow>, i32) {
    let mut ready = 0;
    let rows = queue_items(ch)
        .into_iter()
        .map(|it| {
            let file: SharedString = it.file_name().into();
            let (title, status, tone) = match &it.sheet {
                None => ("Missing details file".to_string(), "No sheet", 3),
                Some(p) => match read_sheet(p) {
                    Ok(s) => match s.title {
                        Some(t) if is_settling(&it.video, SETTLE) => (t, "Copying…", 2),
                        Some(t) => {
                            ready += 1;
                            (t, "Queued", 0)
                        }
                        None => ("Sheet without title".to_string(), "Invalid", 3),
                    },
                    Err(e) => (e, "Invalid", 3),
                },
            };
            QueueRow {
                file,
                title: title.into(),
                when: "—".into(),
                status: status.into(),
                tone,
            }
        })
        .collect();
    (rows, ready)
}

fn reload(ui: &AppWindow) {
    let (home, source) = data_root();
    let names = list_channels(&home);
    let rows: Vec<ChannelRow> = names
        .iter()
        .map(|n| match load_channel(n, &home) {
            Ok(ch) => {
                let q = queue_items(&ch).len();
                let connected = auth::read_token(&ch.profile).is_some();
                let state = if connected {
                    "connected"
                } else {
                    "not connected"
                };
                ChannelRow {
                    name: n.into(),
                    subtitle: format!("{q} queued · {state}").into(),
                    connected,
                }
            }
            Err(_) => ChannelRow {
                name: n.into(),
                subtitle: "invalid channel.yaml".into(),
                connected: false,
            },
        })
        .collect();
    ui.set_channels(model(rows));
    if demo::active() {
        // Shown without the user's own temp path, so demo screenshots carry no personal details.
        let shown = if cfg!(windows) {
            "%TEMP%\\AutoUpload-demo\\data"
        } else {
            "$TMPDIR/AutoUpload-demo/data"
        };
        ui.set_data_folder(shown.into());
        ui.set_data_source("demo mode: sample data, offline".into());
    } else {
        ui.set_data_folder(home.display().to_string().into());
        ui.set_data_source(source.describe().into());
    }
    let mut current = ui.get_current();
    if current >= names.len() as i32 {
        current = names.len() as i32 - 1;
    }
    if current < 0 && !names.is_empty() {
        current = 0;
    }
    ui.set_current(current);
    CTX.with(|c| {
        let mut c = c.borrow_mut();
        c.home = home.clone();
        c.names = names.clone();
    });
    show_channel(ui, current);
    refresh_activity(ui, true);
}

fn show_channel(ui: &AppWindow, index: i32) {
    let (home, names) = CTX.with(|c| (c.borrow().home.clone(), c.borrow().names.clone()));
    let Some(name) = usize::try_from(index).ok().and_then(|i| names.get(i)) else {
        CTX.with(|c| c.borrow_mut().channel = None);
        return;
    };
    match load_channel(name, &home) {
        Ok(ch) => {
            ui.set_ch_name(ch.name.clone().into());
            ui.set_ch_schedule(ch.schedule_text().into());
            ui.set_ch_linked(
                ch.expected_channel_id()
                    .map(|id| format!("YouTube {id}"))
                    .unwrap_or_default()
                    .into(),
            );
            ui.set_ch_connected(auth::read_token(&ch.profile).is_some());
            let (rows, ready) = disk_rows(&ch);
            ui.set_queue(model(rows));
            ui.set_ready_count(ready);
            ui.set_calendar(model(vec![]));
            ui.set_calendar_note("".into());
            CTX.with(|c| c.borrow_mut().channel = Some(ch));
        }
        Err(e) => {
            ui.set_ch_name(name.into());
            ui.set_ch_schedule("invalid channel.yaml".into());
            ui.set_ch_linked("".into());
            ui.set_ch_connected(false);
            ui.set_queue(model(vec![]));
            ui.set_ready_count(0);
            banner(ui, e, 3);
            CTX.with(|c| c.borrow_mut().channel = None);
        }
    }
}

fn current_channel() -> Option<Channel> {
    CTX.with(|c| c.borrow().channel.clone())
}

fn set_busy(ui: &AppWindow, busy: bool, status: &str) {
    ui.set_busy(busy);
    ui.set_progress(-1.0);
    ui.set_status(status.into());
}

/// Runs `work` on a thread, then `done` on the UI thread.
fn spawn<T: Send + 'static>(
    weak: &Weak<AppWindow>,
    work: impl FnOnce(&Weak<AppWindow>) -> T + Send + 'static,
    done: impl FnOnce(&AppWindow, T) + Send + 'static,
) {
    let weak = weak.clone();
    std::thread::spawn(move || {
        let out = work(&weak);
        let _ = weak.upgrade_in_event_loop(move |ui| done(&ui, out));
    });
}

fn with_ui(weak: &Weak<AppWindow>, f: impl FnOnce(&AppWindow) + Send + 'static) {
    let _ = weak.upgrade_in_event_loop(move |ui| f(&ui));
}

/// Updates the queue rows of `file` in place from a worker.
fn update_row(rows: &mut [QueueRow], file: &str, when: Option<&str>, status: &str, tone: i32) {
    if let Some(r) = rows.iter_mut().find(|r| r.file.as_str() == file) {
        if let Some(w) = when {
            r.when = w.into();
        }
        r.status = status.into();
        r.tone = tone;
    }
}

fn run_queue(weak: &Weak<AppWindow>, ch: Channel, execute: bool, allow_public: bool) {
    let (rows, _) = disk_rows(&ch);
    let name = ch.name.clone();
    spawn(
        weak,
        move |weak| {
            let api = match api_for(&ch) {
                Ok(a) => a,
                Err(e) => return Err(e),
            };
            let mut rows = rows;
            let mut current = String::new();
            let opts = RunOptions {
                execute,
                allow_public,
                ..Default::default()
            };
            let mut on_event = |e: Event| {
                let mut line = None;
                let mut progress = None;
                match &e {
                    Event::Info(s) => line = Some(s.clone()),
                    Event::Planned { file, title, when } => {
                        update_row(
                            &mut rows,
                            file,
                            Some(when),
                            if execute { "Waiting" } else { "Planned" },
                            0,
                        );
                        line = Some(format!("{file} → '{title}' · {when}"));
                    }
                    Event::Uploading { file } => {
                        current = file.clone();
                        update_row(&mut rows, file, None, "Uploading", 4);
                        line = Some(format!("uploading {file}"));
                        progress = Some(0.0);
                    }
                    Event::Progress { fraction, .. } => progress = Some(*fraction),
                    Event::Done {
                        file,
                        url,
                        verified,
                        ..
                    } => {
                        update_row(
                            &mut rows,
                            file,
                            None,
                            if *verified {
                                "Uploaded"
                            } else {
                                "Check in Studio"
                            },
                            if *verified { 1 } else { 2 },
                        );
                        line = Some(format!("✓ {file}: {url}"));
                    }
                    Event::Warning(s) => line = Some(format!("! {s}")),
                    Event::Error(s) => {
                        if let Some(file) = s.split(':').next() {
                            update_row(&mut rows, file, None, "Error", 3);
                        }
                        line = Some(format!("✗ {s}"));
                    }
                }
                let snapshot = rows.clone();
                let file = current.clone();
                with_ui(weak, move |ui| {
                    ui.set_queue(model(snapshot));
                    if let Some(l) = line {
                        log_line(ui, l);
                    }
                    if let Some(p) = progress {
                        ui.set_progress(p);
                        ui.set_status(format!("Uploading {file} — {:.0}%", p * 100.0).into());
                    }
                });
            };
            process_queue(&ch, &*api, &opts, &mut on_event)
                .map(|r| (r, rows))
                .map_err(|s| s.message)
        },
        move |ui, out| {
            set_busy(ui, false, "");
            let action = if execute { "upload" } else { "plan" };
            match &out {
                Ok((r, _)) => {
                    let summary = if execute {
                        format!("Uploaded {} video(s)", r.done.len())
                    } else {
                        format!("Previewed {} date(s)", r.plan.len())
                    };
                    let detail = r
                        .errors
                        .first()
                        .map(|e| format!("; {e}"))
                        .unwrap_or_default();
                    record(
                        ui,
                        Some(&name),
                        action,
                        summary + &detail,
                        r.errors.is_empty(),
                    );
                }
                Err(e) => record(ui, Some(&name), action, format!("Stopped: {e}"), false),
            }
            match out {
                Ok((r, rows)) => {
                    if execute {
                        let n = r.done.len();
                        let msg = if r.errors.is_empty() {
                            format!("{n} video(s) uploaded and scheduled.")
                        } else {
                            format!(
                                "{n} video(s) uploaded; {} problem(s). See Activity.",
                                r.errors.len()
                            )
                        };
                        banner(ui, msg, if r.errors.is_empty() { 1 } else { 2 });
                        reload(ui);
                    } else {
                        ui.set_queue(model(rows));
                        if !r.errors.is_empty() {
                            banner(
                                ui,
                                format!(
                                    "{} video(s) cannot be uploaded yet. See Activity.",
                                    r.errors.len()
                                ),
                                2,
                            );
                        }
                    }
                }
                Err(e) => {
                    log_line(ui, format!("✗ {e}"));
                    banner(ui, e, 3);
                    reload(ui);
                }
            }
        },
    );
}

fn main() -> Result<(), slint::PlatformError> {
    // `--demo`: sample channels and an offline YouTube; must be set before any thread starts.
    if std::env::args().any(|a| a == "--demo") || demo::active() {
        if let Err(e) = demo::enable() {
            eprintln!("Could not prepare the demo: {e}");
        }
    }
    let ui = AppWindow::new()?;
    ui.set_version(autoupload_core::VERSION.into());
    ui.set_demo(demo::active());
    let settings = Settings::load();
    ui.set_saved_theme(settings.theme.unwrap_or_default().into());
    ui.set_agents_may_upload(settings.agents_may_upload);
    agent_instructions(&ui);
    reload(&ui);
    refresh_activity(&ui, true);
    let weak = ui.as_weak();

    // Agents and the daily task write to the activity log from other processes: pick their entries up.
    let activity_timer = Timer::default();
    activity_timer.start(TimerMode::Repeated, Duration::from_secs(2), {
        let weak = weak.clone();
        move || {
            if let Some(ui) = weak.upgrade() {
                refresh_activity(&ui, false);
            }
        }
    });

    ui.on_set_agents_may_upload({
        let weak = weak.clone();
        move |on| {
            let ui = weak.unwrap();
            let mut s = Settings::load();
            s.agents_may_upload = on;
            match s.save() {
                Ok(()) => record(
                    &ui,
                    None,
                    "permission",
                    if on {
                        "Allowed AI agents to upload"
                    } else {
                        "AI agents may no longer upload"
                    },
                    true,
                ),
                Err(e) => {
                    ui.set_agents_may_upload(!on);
                    banner(&ui, format!("Could not save settings: {e}"), 3);
                }
            }
        }
    });

    ui.on_theme_changed(|dark| {
        let mut s = Settings::load();
        s.theme = Some(if dark { "dark" } else { "light" }.to_string());
        let _ = s.save();
    });

    ui.on_select_channel({
        let weak = weak.clone();
        move |i| {
            let ui = weak.unwrap();
            ui.set_current(i);
            ui.set_tab(0);
            show_channel(&ui, i);
        }
    });

    ui.on_create_channel({
        let weak = weak.clone();
        move |name| {
            let ui = weak.unwrap();
            let home = CTX.with(|c| c.borrow().home.clone());
            match create_channel(name.trim(), &home) {
                Ok(folder) => {
                    log_line(&ui, format!("channel created in {}", folder.display()));
                    record(&ui, Some(name.trim()), "create_channel", format!("Created channel {}", name.trim()), true);
                    reload(&ui);
                    let idx = CTX.with(|c| c.borrow().names.iter().position(|n| n == name.trim()));
                    if let Some(i) = idx {
                        ui.set_current(i as i32);
                        show_channel(&ui, i as i32);
                    }
                    banner(&ui, "Channel created. Review its settings (hours and time zone), then Connect it with Google.", 1);
                }
                Err(e) => banner(&ui, e, 3),
            }
        }
    });

    ui.on_connect({
        let weak = weak.clone();
        move || {
            let ui = weak.unwrap();
            let Some(ch) = current_channel() else { return };
            if demo::active() {
                banner(&ui, "Demo mode: the sample channels are already connected. Nothing is sent to Google.", 1);
                return;
            }
            set_busy(
                &ui,
                true,
                "Waiting for you to sign in with Google in your browser…",
            );
            let name = ch.name.clone();
            log_line(&ui, format!("[{}] signing in", ch.name));
            spawn(
                &weak,
                move |_| -> Result<String, String> {
                    auth::login(&ch.client_secret, &ch.profile, &|url| {
                        let _ = open::that(url);
                    })
                    .map_err(|e| e.0)?;
                    let yt = api_for(&ch)?.my_channel().map_err(|e| e.to_string())?;
                    write_atomic(&ch.channel_id_file(), &yt.id).map_err(|e| e.to_string())?;
                    Ok(format!("Connected to {} ({})", yt.title, yt.id))
                },
                move |ui, out| {
                    set_busy(ui, false, "");
                    match &out {
                        Ok(msg) => record(ui, Some(&name), "connect", msg.clone(), true),
                        Err(e) => record(ui, Some(&name), "connect", format!("Sign-in failed: {e}"), false),
                    }
                    match out {
                        Ok(msg) => {
                            log_line(ui, &msg);
                            banner(ui, msg, 1);
                        }
                        Err(e) => {
                            log_line(ui, format!("✗ {e}"));
                            banner(ui, e, 3);
                        }
                    }
                    reload(ui);
                },
            );
        }
    });

    ui.on_disconnect({
        let weak = weak.clone();
        move || {
            let ui = weak.unwrap();
            let Some(ch) = current_channel() else { return };
            set_busy(&ui, true, "Signing out…");
            let name = ch.name.clone();
            spawn(
                &weak,
                move |_| auth::logout(&ch.profile, true),
                move |ui, had| {
                    set_busy(ui, false, "");
                    let msg = if had {
                        "Signed out: access revoked at Google and the local sign-in deleted."
                    } else {
                        "There was no session."
                    };
                    log_line(ui, msg);
                    record(ui, Some(&name), "disconnect", "Signed out", true);
                    banner(ui, msg, 0);
                    reload(ui);
                },
            );
        }
    });

    ui.on_preview({
        let weak = weak.clone();
        move || {
            let ui = weak.unwrap();
            let Some(ch) = current_channel() else { return };
            set_busy(&ui, true, "Reading the channel calendar…");
            run_queue(&weak, ch, false, false);
        }
    });

    ui.on_upload({
        let weak = weak.clone();
        move |allow_public| {
            let ui = weak.unwrap();
            let Some(ch) = current_channel() else { return };
            set_busy(&ui, true, "Starting the upload…");
            run_queue(&weak, ch, true, allow_public);
        }
    });

    ui.on_load_calendar({
        let weak = weak.clone();
        move || {
            let ui = weak.unwrap();
            let Some(ch) = current_channel() else { return };
            if !ui.get_ch_connected() {
                ui.set_calendar_note("Connect the channel to see its calendar.".into());
                return;
            }
            set_busy(&ui, true, "Loading the calendar…");
            let zone = ch.zone;
            spawn(
                &weak,
                move |_| -> Result<Vec<CalendarRow>, String> {
                    let api = api_for(&ch)?;
                    let rows = calendar_rows(&*api, 30, Utc::now()).map_err(|e| e.to_string())?;
                    Ok(rows
                        .into_iter()
                        .map(|(when, privacy, title)| CalendarRow {
                            when: zone.local(when).format("%a %d %b · %H:%M").to_string().into(),
                            status: if privacy == "private" { "scheduled".into() } else { privacy.into() },
                            title: title.into(),
                        })
                        .collect())
                },
                move |ui, out| {
                    set_busy(ui, false, "");
                    match out {
                        Ok(rows) => {
                            let n = rows.len();
                            ui.set_calendar(model(rows));
                            ui.set_calendar_note(format!("{n} upload(s) from 3 days ago to 30 days ahead, in the channel's time zone.").into());
                        }
                        Err(e) => {
                            ui.set_calendar_note(e.clone().into());
                            banner(ui, e, 3);
                        }
                    }
                },
            );
        }
    });

    ui.on_open_queue(move || {
        if let Some(ch) = current_channel() {
            let _ = std::fs::create_dir_all(ch.queue());
            let _ = open::that(ch.queue());
        }
    });
    ui.on_open_uploaded(move || {
        if let Some(ch) = current_channel() {
            let _ = std::fs::create_dir_all(ch.uploaded());
            let _ = open::that(ch.uploaded());
        }
    });
    ui.on_edit_settings(move || {
        if let Some(ch) = current_channel() {
            let _ = open::that(ch.config_file());
        }
    });
    ui.on_open_data_folder(move || {
        let home = CTX.with(|c| c.borrow().home.clone());
        let _ = std::fs::create_dir_all(&home);
        let _ = open::that(home);
    });
    ui.on_open_url(move |url| {
        let _ = open::that(url.as_str());
    });

    ui.on_set_data_folder({
        let weak = weak.clone();
        move |path| {
            let ui = weak.unwrap();
            let p = PathBuf::from(path.trim().trim_matches('"'));
            if !p.is_absolute() {
                banner(
                    &ui,
                    "Use the full path of the folder, e.g. C:\\AutoUpload",
                    3,
                );
                return;
            }
            let mut s = Settings::load();
            s.data_folder = Some(p.clone());
            match s.save() {
                Ok(()) => {
                    ui.set_current(-1);
                    reload(&ui);
                    record(
                        &ui,
                        None,
                        "data_folder",
                        format!("Data folder set to {}", demo_or_real(&p)),
                        true,
                    );
                    banner(&ui, format!("Data folder: {}", p.display()), 1);
                }
                Err(e) => banner(&ui, format!("Could not save settings: {e}"), 3),
            }
        }
    });

    ui.on_run_doctor({
        let weak = weak.clone();
        move || {
            let ui = weak.unwrap();
            let rows: Vec<CheckRow> = doctor::run()
                .into_iter()
                .map(|c| {
                    let show_fix = c.level != Level::Ok || c.message.starts_with("note:");
                    CheckRow {
                        level: match c.level {
                            Level::Ok => 0,
                            Level::Warning => 1,
                            Level::Error => 2,
                        },
                        fix: if show_fix {
                            c.fix.into()
                        } else {
                            SharedString::new()
                        },
                        message: c.message.into(),
                    }
                })
                .collect();
            ui.set_checks(model(rows));
            ui.set_show_doctor(true);
        }
    });

    ui.on_dismiss_banner({
        let weak = weak.clone();
        move || weak.unwrap().set_banner("".into())
    });

    ui.run()
}
