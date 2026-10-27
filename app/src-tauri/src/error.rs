use serde::{Serialize, Serializer};

/// Errors returned to the frontend as plain messages.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("no game installation is open")]
    NoGame,
    #[error("no truck is loaded")]
    NoTruck,
    #[error(transparent)]
    GameData(#[from] game_data::GameDataError),
    #[error(transparent)]
    Config(#[from] tuning_core::ConfigError),
    #[error("background task failed: {0}")]
    Task(String),
    #[error("settings: {0}")]
    Settings(String),
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}
