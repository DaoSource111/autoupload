//! `autoupload`: the command line. Nothing is uploaded without --yes; nothing is made public without --allow-public.

use autoupload_core::channel::{create_channel, list_channels, load_channel, Channel};
use autoupload_core::exit::{AUTH, INVALID, OK, REMOTE};
use autoupload_core::paths::{data_root, Settings};
use autoupload_core::runner::{calendar_rows, process_queue, Event, RunOptions};
use autoupload_core::youtube::{Api, YouTube};
use autoupload_core::{auth, doctor, fsutil, queue};
use chrono::Utc;
use clap::{Parser, Subcommand};
use std::io::Write;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "autoupload",
    version,
    about = "Upload and schedule videos on your YouTube channels (official API)"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
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
    /// Check the installation offline and say how to fix each problem
    Doctor,
    /// Show the data folder, or set it: autoupload data-folder <path>
    DataFolder { path: Option<PathBuf> },
}

fn main() {
    std::process::exit(run(Cli::parse()));
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

fn client(ch: &Channel) -> Result<YouTube, String> {
    auth::Session::load(&ch.profile)
        .map(YouTube::new)
        .map_err(|e| e.0)
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

fn run(cli: Cli) -> i32 {
    match cli.cmd {
        Cmd::Channel { name, .. } => match create_channel(&name, &home()) {
            Ok(folder) => {
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
                    auth::login(&ch.client_secret, &ch.profile, &open_browser).map_err(|e| e.0)?;
                    let yt = client(&ch)?.my_channel().map_err(|e| e.to_string())?;
                    if let Some(prev) = ch.expected_channel_id() {
                        if prev != yt.id {
                            println!("Note: this channel used to be {prev}, and you have now signed in as {} ({}).", yt.title, yt.id);
                        }
                    }
                    fsutil::write_atomic(&ch.channel_id_file(), &yt.id)
                        .map_err(|e| e.to_string())?;
                    println!("Connected '{channel}' → {} ({})", yt.title, yt.id);
                    Ok(())
                }
                "logout" => {
                    println!(
                        "{}",
                        if auth::logout(&ch.profile, true) {
                            "Signed out."
                        } else {
                            "There was no session."
                        }
                    );
                    Ok(())
                }
                _ => {
                    let yt = client(&ch)?.my_channel().map_err(|e| e.to_string())?;
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
        Cmd::Calendar { channel, days } => {
            let ch = match load(&channel) {
                Ok(c) => c,
                Err(code) => return code,
            };
            let api = match client(&ch) {
                Ok(a) => a,
                Err(e) => {
                    eprintln!("Authentication: {e}");
                    return AUTH;
                }
            };
            match calendar_rows(&api, days, Utc::now()) {
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
                let api = match client(&ch) {
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
                match process_queue(&ch, &api, &opts, &mut print_event) {
                    Ok(r) => {
                        if !r.errors.is_empty() && code == OK {
                            code = INVALID;
                        }
                        if !yes && !r.plan.is_empty() {
                            println!("[{n}] Nothing was uploaded. Run again with --yes to execute this plan.");
                        }
                    }
                    Err(stop) => {
                        eprintln!("{stop}");
                        code = if stop.auth { AUTH } else { REMOTE };
                    }
                }
            }
            code
        }
        Cmd::Doctor => {
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
                println!("{} ({})", p.display(), src.describe());
                OK
            }
            Some(p) => {
                let abs = if p.is_absolute() {
                    p
                } else {
                    std::env::current_dir().map(|d| d.join(&p)).unwrap_or(p)
                };
                let s = Settings {
                    data_folder: Some(abs.clone()),
                };
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
