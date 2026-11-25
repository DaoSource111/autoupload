//! AutoUpload desktop app (Slint). Network work runs on worker threads; the window never blocks.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use autoupload_core::auth;
use autoupload_core::channel::{create_channel, list_channels, load_channel, Channel};
use autoupload_core::doctor::{self, Level};
use autoupload_core::fsutil::write_atomic;
use autoupload_core::manifest::read_sheet;
use autoupload_core::paths::{data_root, Settings};
use autoupload_core::queue::{is_settling, queue_items, SETTLE};
use autoupload_core::runner::{calendar_rows, process_queue, Event, RunOptions};
use autoupload_core::youtube::{Api, YouTube};
use chrono::Utc;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel, Weak};
use std::cell::RefCell;
use std::path::PathBuf;

slint::include_modules!();

#[derive(Default)]
struct Ctx {
    home: PathBuf,
    names: Vec<String>,
    channel: Option<Channel>,
    log: Vec<SharedString>,
}

thread_local! {
    static CTX: RefCell<Ctx> = RefCell::new(Ctx::default());
}

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(v))
}

fn log_line(ui: &AppWindow, line: impl Into<SharedString>) {
    let stamp = chrono::Local::now().format("%H:%M:%S");
    let line: SharedString = format!("{stamp}  {}", line.into()).into();
    let lines = CTX.with(|c| {
        let mut c = c.borrow_mut();
        c.log.push(line);
        if c.log.len() > 2000 {
            c.log.remove(0);
        }
        c.log.clone()
    });
    ui.set_log(model(lines));
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
    ui.set_data_folder(home.display().to_string().into());
    ui.set_data_source(source.describe().into());
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

fn api_for(ch: &Channel) -> Result<YouTube, String> {
    auth::Session::load(&ch.profile)
        .map(YouTube::new)
        .map_err(|e| e.0)
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
            process_queue(&ch, &api, &opts, &mut on_event)
                .map(|r| (r, rows))
                .map_err(|s| s.message)
        },
        move |ui, out| {
            set_busy(ui, false, "");
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
    let ui = AppWindow::new()?;
    ui.set_version(autoupload_core::VERSION.into());
    ui.set_saved_theme(Settings::load().theme.unwrap_or_default().into());
    reload(&ui);
    let weak = ui.as_weak();

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
            set_busy(
                &ui,
                true,
                "Waiting for you to sign in with Google in your browser…",
            );
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
                    let rows = calendar_rows(&api, 30, Utc::now()).map_err(|e| e.to_string())?;
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
