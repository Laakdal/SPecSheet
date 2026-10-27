//! Persisted user settings: `settings.json` in the app config directory
//! (`%APPDATA%\dev.specsheet.app` on Windows, as in the Tauri builds).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const APP_DIR: &str = "dev.specsheet.app";
const FILE: &str = "settings.json";

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// The last game directory that opened successfully.
    pub game_dir: Option<PathBuf>,
    /// The truck tuned last (`TruckId`).
    pub truck: Option<String>,
}

fn path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join(APP_DIR).join(FILE))
}

/// Loads the settings; a missing or unreadable file yields defaults.
pub fn load() -> Settings {
    path()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save(settings: &Settings) -> Result<(), String> {
    let path = path().ok_or("no config directory")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}
