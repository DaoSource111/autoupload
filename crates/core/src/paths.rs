//! Where AutoUpload keeps things.
//!
//! - Tokens and settings: the config folder (`AUTOUPLOAD_CONFIG_DIR`, else `%APPDATA%\AutoUpload` on Windows,
//!   else `$XDG_CONFIG_HOME/autoupload` or `~/.config/autoupload`), the same place as 1.x.
//! - Channels and the OAuth client file: the data folder (`channels/`, `config/`), resolved by [`data_root`].

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::fsutil::write_atomic;

pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

pub fn config_dir() -> PathBuf {
    if let Some(v) = std::env::var_os("AUTOUPLOAD_CONFIG_DIR") {
        return PathBuf::from(v);
    }
    #[cfg(windows)]
    if let Some(a) = std::env::var_os("APPDATA") {
        return PathBuf::from(a).join("AutoUpload");
    }
    let xdg = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"));
    xdg.join("autoupload")
}

/// Data folder when nothing else is configured: `Documents\AutoUpload` on Windows, `~/AutoUpload` elsewhere.
pub fn default_home() -> PathBuf {
    if cfg!(windows) {
        dirs::document_dir()
            .unwrap_or_else(|| home().join("Documents"))
            .join("AutoUpload")
    } else {
        home().join("AutoUpload")
    }
}

/// User settings kept in the config folder (`settings.json`).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_folder: Option<PathBuf>,
    /// Desktop app colours: "light" or "dark"; unset follows the system.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
}

impl Settings {
    pub fn path() -> PathBuf {
        config_dir().join("settings.json")
    }

    pub fn load() -> Settings {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let text = serde_json::to_string_pretty(self).expect("settings serialize");
        write_atomic(&Self::path(), &text)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootSource {
    /// `AUTOUPLOAD_HOME`
    Env,
    /// `data_folder` in settings.json (set from the app or `autoupload data-folder <path>`)
    Settings,
    /// a `channels/` or `config/` folder next to the program (portable copy)
    Portable,
    /// [`default_home`]
    Default,
}

impl RootSource {
    pub fn describe(self) -> &'static str {
        match self {
            RootSource::Env => "from AUTOUPLOAD_HOME",
            RootSource::Settings => "chosen in settings",
            RootSource::Portable => "next to the program",
            RootSource::Default => "default",
        }
    }
}

fn looks_like_data(dir: &Path) -> bool {
    dir.join("channels").is_dir() || dir.join("config").is_dir()
}

/// Folder that holds `channels/` and `config/`, and why it was chosen.
pub fn data_root() -> (PathBuf, RootSource) {
    if let Some(v) = std::env::var_os("AUTOUPLOAD_HOME") {
        return (PathBuf::from(v), RootSource::Env);
    }
    if let Some(p) = Settings::load().data_folder {
        return (p, RootSource::Settings);
    }
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        if looks_like_data(&dir) {
            return (dir, RootSource::Portable);
        }
    }
    (default_home(), RootSource::Default)
}

pub fn root() -> PathBuf {
    data_root().0
}
