//! AutoUpload core: everything except the user interface.
//!
//! Each video in `channels/<channel>/queue/`, with its YAML sheet, is uploaded as private and scheduled in the
//! channel's next free publishing slot through the YouTube Data API v3, with the user's own OAuth client.
//! The files on disk (channel.yaml, sheets, state.sqlite, tokens) are compatible with AutoUpload 1.x.

pub mod activity;
pub mod auth;
pub mod channel;
pub mod client;
pub mod demo;
pub mod doctor;
pub mod fsutil;
pub mod manifest;
pub mod ops;
pub mod paths;
pub mod publisher;
pub mod queue;
pub mod runner;
pub mod state;
pub mod task;
pub mod validate;
pub mod youtube;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GUIDE_URL: &str = "https://daosource111.github.io/autoupload/install.html";

/// CLI exit codes, unchanged from 1.x.
pub mod exit {
    pub const OK: i32 = 0;
    pub const INVALID: i32 = 2;
    pub const AUTH: i32 = 3;
    pub const REMOTE: i32 = 4;
}
