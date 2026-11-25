//! `autoupload`: the command line. Nothing is uploaded without --yes; nothing is made public without --allow-public.
//!
//! AI agents drive it through `autoupload mcp` (typed tools) or, in a terminal, with `--json` and
//! `AUTOUPLOAD_AGENT=<name>` set, so their actions are logged as theirs and uploads follow the person's permission.

mod mcp;

use autoupload_core::activity::{self, Actor, CLI};
use autoupload_core::channel::{create_channel, list_channels, load_channel, Channel};
use autoupload_core::client::api_for;
use autoupload_core::exit::{AUTH, INVALID, OK, REMOTE};
use autoupload_core::ops::{self, SheetFields};
use autoupload_core::paths::{data_root, Settings};
use autoupload_core::runner::{calendar_rows, process_queue, Event, RunOptions};
use autoupload_core::{auth, demo, doctor, fsutil, queue, task};
use chrono::Utc;
use clap::{Args, Parser, Subcommand};
use serde_json::Value;
use std::io::Write;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "autoupload",
    version,
    about = "Upload and schedule videos on your YouTube channels (official API). Native, offline-first, written in Rust.",
    after_help = "AI agents: run `autoupload agents setup` to connect Claude Code, Codex or another MCP client."
)]
struct Cli {
    /// Print machine-readable JSON (for scripts and AI agents)
    #[arg(long, global = true)]
    json: bool,
    /// Use sample channels and an offline YouTube: nothing is sent anywhere, your data is not touched
    #[arg(long, global = true)]
    demo: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Args)]
struct Details {
    /// Video title (at most 100 characters)
    #[arg(long)]
    title: String,
    #[arg(long)]
    description: Option<String>,
    /// Comma-separated tags
    #[arg(long, value_delimiter = ',')]
    tags: Vec<String>,
    /// Fixed publish time, ISO 8601 with offset; otherwise the next free slot
    #[arg(long)]
    publish_at: Option<String>,
    #[arg(long)]
    playlist_id: Option<String>,
    /// True if the video has realistic AI-generated or altered voice or imagery
    #[arg(long)]
    synthetic: Option<bool>,
}

impl Details {
    fn fields(&self) -> SheetFields {
        SheetFields {
            title: Some(self.title.clone()),
            description: self.description.clone(),
            tags: (!self.tags.is_empty()).then(|| self.tags.clone()),
            publish_at: self.publish_at.clone(),
            playlist_id: self.playlist_id.clone(),
            contains_synthetic_media: self.synthetic,
            ..Default::default()
        }
    }
}

#[derive(Subcommand)]
enum Cmd {
    /// Create a channel: autoupload channel new <name>
    Channel {
        #[arg(value_parser = ["new"])]
        action: String,
        name: String,
    },
    /// List the configured channels and their queue
    Channels,
    /// Connect a channel: autoupload auth <channel> login | whoami | logout
    Auth {
        channel: String,
        #[arg(value_parser = ["login", "whoami", "logout"])]
        action: String,
    },
    /// The videos in a channel's queue and what blocks each one
    Queue { channel: String },
    /// Copy a video into a channel's queue and write its details file
    Add {
        channel: String,
        video: PathBuf,
        #[command(flatten)]
        details: Details,
        /// A .jpg or .png thumbnail to copy next to it
        #[arg(long)]
        thumbnail: Option<PathBuf>,
        /// Move the file instead of copying it
        #[arg(long = "move")]
        move_file: bool,
    },
    /// Write or replace the details file of a video already in the queue
    Sheet {
        channel: String,
        /// File name in the queue, e.g. episode-4.mp4
        file: String,
        #[command(flatten)]
        details: Details,
    },
    /// Scheduled and recent uploads of a channel (read only)
    Calendar {
        channel: String,
        #[arg(long, default_value_t = 30)]
        days: i64,
    },
    /// Upload the queue as private with a date (without --yes it only shows the plan)
    Schedule {
        channel: Option<String>,
        /// All configured channels
        #[arg(long = "all")]
        all: bool,
        /// Confirm that you want to upload to your channel
        #[arg(long)]
        yes: bool,
        /// Maximum number of videos per channel in this run
        #[arg(long)]
        max: Option<usize>,
        /// Allow sheets with privacy: public
        #[arg(long = "allow-public")]
        allow_public: bool,
    },
    /// What the app, the daily task and AI agents did recently
    Activity {
        #[arg(short = 'n', long, default_value_t = 30)]
        limit: usize,
    },
    /// AI agents: setup instructions and upload permission
    Agents {
        #[command(subcommand)]
        action: AgentsCmd,
    },
    /// Run AutoUpload as an MCP server on stdin/stdout (started by an AI agent, not by hand)
    Mcp,
    /// The daily unattended run (Task Scheduler, launchd or systemd)
    Task {
        #[command(subcommand)]
        action: TaskCmd,
    },
    /// Check the installation offline and say how to fix each problem
    Doctor,
    /// Show the data folder, or set it: autoupload data-folder <path>
    DataFolder { path: Option<PathBuf> },
}

#[derive(Subcommand)]
enum AgentsCmd {
    /// How to connect Claude Code, Codex, Cursor or any MCP client
    Setup,
    /// Show what agents may do
    Status,
    /// Let agents upload (on) or only prepare and preview (off, the default)
    AllowUpload {
        #[arg(value_parser = ["on", "off"])]
        state: String,
    },
}

#[derive(Subcommand)]
enum TaskCmd {
    /// Register the daily run with the system scheduler
    Install {
        /// Local time, HH:MM
        #[arg(long, default_value = "10:00")]
        time: String,
    },
    /// Remove the daily run
    Remove,
    /// Show whether the daily run is registered
    Status,
    /// The run itself: upload every channel's queue and write a log (what the scheduler starts)
    Run,
}

/// Double-clicked in Explorer (no arguments, a console of its own): open the desktop app instead of
/// flashing a console window that closes at once. Returns true when it handled the start.
#[cfg(windows)]
fn opened_from_explorer() -> bool {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetConsoleProcessList(list: *mut u32, count: u32) -> u32;
    }
    if std::env::args_os().len() > 1 {
        return false;
    }
    let mut ids = [0u32; 4];
    // SAFETY: the buffer is valid for `ids.len()` entries; the call only writes into it.
    let attached = unsafe { GetConsoleProcessList(ids.as_mut_ptr(), ids.len() as u32) };
    if attached != 1 {
        return false; // started from a terminal, or with no console at all
    }
    let app = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("autoupload-gui.exe")));
    if let Some(app) = app.filter(|p| p.is_file()) {
        if std::process::Command::new(app).spawn().is_ok() {
            return true;
        }
    }
    println!(
        "This is the AutoUpload command line, for terminals and AI agents.\n\
         To use the desktop app, open autoupload-gui.exe. To see the commands, run `autoupload --help`\n\
         in a terminal.\n\nPress Enter to close."
    );
    let _ = std::io::stdin().read_line(&mut String::new());
    true
}

fn main() {
    #[cfg(windows)]
    if opened_from_explorer() {
        return;
    }
    let cli = Cli::parse();
    if cli.demo || demo::active() {
        match demo::enable() {
            Ok(root) => eprintln!(
                "Demo mode: sample data in {} — nothing is sent to YouTube.",
                root.display()
            ),
            Err(e) => {
                eprintln!("Could not prepare the demo: {e}");
                std::process::exit(INVALID);
            }
        }
    }
    std::process::exit(run(cli));
}

fn home() -> PathBuf {
    data_root().0
}

fn load(name: &str) -> Result<Channel, i32> {
    load_channel(name, &home()).map_err(|e| {
        eprintln!("Error: {e}");
        INVALID
    })
}

fn open_browser(url: &str) {
    println!("Opening your browser to sign in with Google. If it does not open, visit:\n{url}\n");
    let _ = open::that(url);
}

fn print_event(e: Event) {
    match e {
        Event::Info(s) => println!("{s}"),
        Event::Planned { file, title, when } => println!("  · {file} → '{title}' · {when}"),
        Event::Uploading { file } => println!("  ↑ {file}"),
        Event::Progress { fraction, .. } => {
            print!("\r    uploading… {:5.1}%", fraction * 100.0);
            let _ = std::io::stdout().flush();
        }
        Event::Done { url, verified, .. } => println!(
            "\n  ✓ {url} ({})",
            if verified {
                "verified"
            } else {
                "check it in YouTube Studio"
            }
        ),
        Event::Warning(s) => println!("\n  ! {s}"),
        Event::Error(s) => println!("\n  ✗ {s}"),
    }
}

/// Prints a JSON result (or error) and returns the exit code.
fn emit(res: Result<Value, String>, err_code: i32) -> i32 {
    match res {
        Ok(v) => {
            println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
            OK
        }
        Err(e) => {
            println!("{}", serde_json::json!({"error": e}));
            err_code
        }
    }
}

fn agents_setup() {
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "autoupload".into());
    let may = Settings::load().agents_may_upload;
    println!(
        r#"AutoUpload speaks MCP, the protocol AI agents use for tools. Register it once in your agent:

  Claude Code     claude mcp add autoupload -- "{exe}" mcp
  Codex CLI       add to ~/.codex/config.toml:
                    [mcp_servers.autoupload]
                    command = "{exe}"
                    args = ["mcp"]
  Cursor, Claude Desktop, VS Code and other MCP clients: a stdio server with
                    command "{exe}", arguments ["mcp"]

Then ask, for example: "Queue every video in D:\Exports for lumen-lab with a title, description and tags,
and preview the dates."

Without MCP, an agent can use the terminal: set AUTOUPLOAD_AGENT=<its name> and add --json to commands.

Agents may always read, queue videos, write details and preview dates. Uploading is {}.
Change it in the app (AI agents panel) or with: autoupload agents allow-upload on|off
Everything agents do appears in the app's Activity tab and in: autoupload activity"#,
        if may {
            "ALLOWED"
        } else {
            "OFF (the person presses Upload)"
        }
    );
}

fn run(cli: Cli) -> i32 {
    let json = cli.json;
    let actor = Actor::from_env(CLI);
    match cli.cmd {
        Cmd::Channel { name, .. } => match create_channel(&name, &home()) {
            Ok(folder) => {
                actor.record(
                    &home(),
                    Some(&name),
                    "create_channel",
                    format!("Created channel {name}"),
                    true,
                );
                println!(
                    "Channel created in {}\n1) Review {}\n2) Connect it: autoupload auth {name} login",
                    folder.display(),
                    folder.join("channel.yaml").display()
                );
                OK
            }
            Err(e) => {
                eprintln!("{e}");
                INVALID
            }
        },
        Cmd::Channels => {
            let base = home();
            if json {
                return emit(Ok(ops::status(&base)), INVALID);
            }
            let names = list_channels(&base);
            if names.is_empty() {
                println!(
                    "No channels in {}. Create one with: autoupload channel new <name>",
                    base.display()
                );
            }
            for n in names {
                match load_channel(&n, &base) {
                    Ok(ch) => {
                        let connected = if auth::read_token(&ch.profile).is_some() {
                            "connected"
                        } else {
                            "NOT CONNECTED"
                        };
                        println!(
                            "{n:<16} queue: {:>3}   {connected}   {}",
                            queue::queue_items(&ch).len(),
                            ch.schedule_text()
                        );
                    }
                    Err(e) => println!("{n:<16} invalid channel.yaml: {e}"),
                }
            }
            OK
        }
        Cmd::Auth { channel, action } => {
            let ch = match load(&channel) {
                Ok(c) => c,
                Err(code) => return code,
            };
            let res: Result<(), String> = (|| match action.as_str() {
                "login" => {
                    if demo::active() {
                        println!("Demo mode: the sample channels are already connected.");
                        return Ok(());
                    }
                    auth::login(&ch.client_secret, &ch.profile, &open_browser).map_err(|e| e.0)?;
                    let yt = api_for(&ch)?.my_channel().map_err(|e| e.to_string())?;
                    if let Some(prev) = ch.expected_channel_id() {
                        if prev != yt.id {
                            println!(
                                "Note: this channel used to be {prev}, and you have now signed in as {} ({}).",
                                yt.title, yt.id
                            );
                        }
                    }
                    fsutil::write_atomic(&ch.channel_id_file(), &yt.id)
                        .map_err(|e| e.to_string())?;
                    actor.record(
                        &home(),
                        Some(&channel),
                        "connect",
                        format!("Connected to {}", yt.title),
                        true,
                    );
                    println!("Connected '{channel}' → {} ({})", yt.title, yt.id);
                    Ok(())
                }
                "logout" => {
                    let had = auth::logout(&ch.profile, true);
                    actor.record(&home(), Some(&channel), "disconnect", "Signed out", true);
                    println!(
                        "{}",
                        if had {
                            "Signed out."
                        } else {
                            "There was no session."
                        }
                    );
                    Ok(())
                }
                _ => {
                    let yt = api_for(&ch)?.my_channel().map_err(|e| e.to_string())?;
                    println!("'{channel}' → {} ({})", yt.title, yt.id);
                    Ok(())
                }
            })();
            match res {
                Ok(()) => OK,
                Err(e) => {
                    eprintln!("Authentication: {e}");
                    AUTH
                }
            }
        }
        Cmd::Queue { channel } => {
            let res = ops::queue(&home(), &actor, &channel);
            if json {
                return emit(res, INVALID);
            }
            match res {
                Ok(v) => {
                    for it in v["items"].as_array().into_iter().flatten() {
                        let state = match (&it["problem"], it["still_copying"].as_bool()) {
                            (Value::String(p), _) => format!("✗ {p}"),
                            (_, Some(true)) => "… still copying".into(),
                            _ => format!("✓ {}", it["title"].as_str().unwrap_or("")),
                        };
                        println!("{:<40} {state}", it["file"].as_str().unwrap_or(""));
                    }
                    println!("{} ready.", v["ready"]);
                    OK
                }
                Err(e) => {
                    eprintln!("{e}");
                    INVALID
                }
            }
        }
        Cmd::Add {
            channel,
            video,
            details,
            thumbnail,
            move_file,
        } => {
            let thumb = thumbnail.map(|t| ops::absolute(&t));
            let res = ops::add_video(
                &home(),
                &actor,
                &channel,
                &ops::absolute(&video),
                &details.fields(),
                move_file,
                thumb.as_deref(),
            );
            if json {
                return emit(res, INVALID);
            }
            match res {
                Ok(v) => {
                    println!("Queued: {}", v["queued"].as_str().unwrap_or(""));
                    OK
                }
                Err(e) => {
                    eprintln!("{e}");
                    INVALID
                }
            }
        }
        Cmd::Sheet {
            channel,
            file,
            details,
        } => {
            let res = ops::write_sheet(&home(), &actor, &channel, &file, &details.fields());
            if json {
                return emit(res, INVALID);
            }
            match res {
                Ok(v) => {
                    println!("Wrote {}", v["sheet"].as_str().unwrap_or(""));
                    OK
                }
                Err(e) => {
                    eprintln!("{e}");
                    INVALID
                }
            }
        }
        Cmd::Calendar { channel, days } => {
            if json {
                return emit(ops::calendar(&home(), &actor, &channel, days), REMOTE);
            }
            let ch = match load(&channel) {
                Ok(c) => c,
                Err(code) => return code,
            };
            let api = match api_for(&ch) {
                Ok(a) => a,
                Err(e) => {
                    eprintln!("Authentication: {e}");
                    return AUTH;
                }
            };
            match calendar_rows(&*api, days, Utc::now()) {
                Ok(rows) => {
                    for (when, privacy, title) in &rows {
                        let label = if privacy == "private" {
                            "scheduled"
                        } else {
                            privacy.as_str()
                        };
                        println!(
                            "{}  {label:<10}  {title}",
                            ch.zone.local(*when).format("%Y-%m-%d %H:%M")
                        );
                    }
                    println!(
                        "{} uploads (time {}). Local queue: {} video(s).",
                        rows.len(),
                        ch.cfg.timezone,
                        queue::queue_items(&ch).len()
                    );
                    OK
                }
                Err(e) => {
                    eprintln!("{e}");
                    REMOTE
                }
            }
        }
        Cmd::Schedule {
            channel,
            all,
            yes,
            max,
            allow_public,
        } => {
            let base = home();
            let names = if all {
                list_channels(&base)
            } else {
                channel.into_iter().collect()
            };
            if names.is_empty() {
                eprintln!("Give a channel or use --all.");
                return INVALID;
            }
            // Agents (and --json) go through the shared operation, which applies the upload permission.
            if json || actor.is_agent() {
                if allow_public {
                    eprintln!("--allow-public is not available to agents or with --json.");
                    return INVALID;
                }
                let mut code = OK;
                for n in names {
                    let c = emit(ops::run(&base, &actor, &n, yes, max), REMOTE);
                    code = code.max(c);
                }
                return code;
            }
            let mut code = OK;
            for n in names {
                let ch = match load(&n) {
                    Ok(c) => c,
                    Err(c) => {
                        code = c;
                        continue;
                    }
                };
                if queue::queue_items(&ch).is_empty() {
                    println!("[{n}] queue empty");
                    continue;
                }
                let api = match api_for(&ch) {
                    Ok(a) => a,
                    Err(e) => {
                        eprintln!("[{n}] Authentication: {e}");
                        code = AUTH;
                        continue;
                    }
                };
                let opts = RunOptions {
                    execute: yes,
                    limit: max,
                    allow_public,
                    ..Default::default()
                };
                let action = if yes { "upload" } else { "plan" };
                match process_queue(&ch, &*api, &opts, &mut print_event) {
                    Ok(r) => {
                        if !r.errors.is_empty() && code == OK {
                            code = INVALID;
                        }
                        let summary = if yes {
                            format!("Uploaded {} video(s)", r.done.len())
                        } else {
                            format!("Previewed {} date(s)", r.plan.len())
                        };
                        actor.record(&base, Some(&n), action, summary, r.errors.is_empty());
                        if !yes && !r.plan.is_empty() {
                            println!(
                                "[{n}] Nothing was uploaded. Run again with --yes to execute this plan."
                            );
                        }
                    }
                    Err(stop) => {
                        actor.record(&base, Some(&n), action, format!("Stopped: {stop}"), false);
                        eprintln!("{stop}");
                        code = if stop.auth { AUTH } else { REMOTE };
                    }
                }
            }
            code
        }
        Cmd::Activity { limit } => {
            let base = home();
            if json {
                return emit(Ok(ops::activity_json(&base, limit)), INVALID);
            }
            let entries = activity::recent(&base, limit);
            if entries.is_empty() {
                println!("No activity yet in {}.", base.display());
            }
            for e in entries {
                println!(
                    "{}  {:<20} {:<14} {} {}",
                    e.time
                        .with_timezone(&chrono::Local)
                        .format("%Y-%m-%d %H:%M"),
                    activity::who(&e),
                    e.channel.as_deref().unwrap_or("-"),
                    if e.ok { "✓" } else { "✗" },
                    e.summary
                );
            }
            OK
        }
        Cmd::Agents { action } => match action {
            AgentsCmd::Setup => {
                agents_setup();
                OK
            }
            AgentsCmd::Status => {
                let may = Settings::load().agents_may_upload;
                if json {
                    return emit(Ok(serde_json::json!({"agents_may_upload": may})), INVALID);
                }
                println!(
                    "AI agents may read, queue videos, write details and preview dates. Uploading: {}.",
                    if may { "allowed" } else { "off" }
                );
                OK
            }
            AgentsCmd::AllowUpload { state } => {
                if actor.is_agent() {
                    eprintln!("Only a person can change what agents may do.");
                    return INVALID;
                }
                let mut s = Settings::load();
                s.agents_may_upload = state == "on";
                match s.save() {
                    Ok(()) => {
                        actor.record(
                            &home(),
                            None,
                            "permission",
                            if s.agents_may_upload {
                                "Allowed AI agents to upload"
                            } else {
                                "AI agents may no longer upload"
                            },
                            true,
                        );
                        println!(
                            "AI agents may {}upload.",
                            if s.agents_may_upload {
                                ""
                            } else {
                                "no longer "
                            }
                        );
                        OK
                    }
                    Err(e) => {
                        eprintln!("Could not save settings: {e}");
                        INVALID
                    }
                }
            }
        },
        Cmd::Mcp => mcp::serve(),
        Cmd::Task { action } => match action {
            TaskCmd::Install { time } => match task::parse_time(&time).and_then(task::install) {
                Ok(msg) => {
                    println!("{msg}");
                    OK
                }
                Err(e) => {
                    eprintln!("{e}");
                    INVALID
                }
            },
            TaskCmd::Remove => match task::remove() {
                Ok(msg) => {
                    println!("{msg}");
                    OK
                }
                Err(e) => {
                    eprintln!("{e}");
                    INVALID
                }
            },
            TaskCmd::Status => {
                println!("{}", task::status());
                OK
            }
            TaskCmd::Run => {
                let (log, ok) = task::run_daily(&home());
                println!("Log: {}", log.display());
                if ok {
                    OK
                } else {
                    REMOTE
                }
            }
        },
        Cmd::Doctor => {
            if json {
                return emit(Ok(ops::doctor_json()), INVALID);
            }
            let checks = doctor::run();
            println!("{}", doctor::render(&checks));
            if doctor::has_errors(&checks) {
                INVALID
            } else {
                OK
            }
        }
        Cmd::DataFolder { path } => match path {
            None => {
                let (p, src) = data_root();
                if json {
                    return emit(
                        Ok(serde_json::json!({"data_folder": p, "source": src.describe()})),
                        INVALID,
                    );
                }
                println!("{} ({})", p.display(), src.describe());
                OK
            }
            Some(p) => {
                let abs = ops::absolute(&p);
                let mut s = Settings::load();
                s.data_folder = Some(abs.clone());
                match s.save() {
                    Ok(()) => {
                        println!("Data folder set to {}", abs.display());
                        OK
                    }
                    Err(e) => {
                        eprintln!("Could not save settings: {e}");
                        INVALID
                    }
                }
            }
        },
    }
}
