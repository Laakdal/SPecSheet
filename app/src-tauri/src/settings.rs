//! Persisted user settings (`settings.json` in the app config directory).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};

use crate::error::AppError;

const FILE: &str = "settings.json";

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// The last game directory that opened successfully.
    pub game_dir: Option<PathBuf>,
}

fn path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, AppError> {
    Ok(app
        .path()
        .app_config_dir()
        .map_err(|e| AppError::Settings(e.to_string()))?
        .join(FILE))
}

/// Loads the settings; a missing or unreadable file yields defaults.
pub fn load<R: Runtime>(app: &AppHandle<R>) -> Settings {
    path(app)
        .ok()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save<R: Runtime>(app: &AppHandle<R>, settings: &Settings) -> Result<(), AppError> {
    let path = path(app)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| AppError::Settings(e.to_string()))?;
    }
    let json =
        serde_json::to_vec_pretty(settings).map_err(|e| AppError::Settings(e.to_string()))?;
    std::fs::write(path, json).map_err(|e| AppError::Settings(e.to_string()))
}
