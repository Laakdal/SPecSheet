use std::path::PathBuf;

/// Errors raised while loading game data.
#[derive(Debug, thiserror::Error)]
pub enum GameDataError {
    #[error("no ETS2 installation found; set ETS2_DIR to the game directory")]
    InstallNotFound,
    #[error("{path} is not an ETS2 installation: {reason}")]
    NotAnInstall { path: PathBuf, reason: String },
    #[error("I/O error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Archive(#[from] specsheet_archive::ArchiveError),
    #[error(transparent)]
    Sii(#[from] specsheet_formats::sii::SiiError),
    #[error("{path}: {source}")]
    Format {
        path: String,
        #[source]
        source: specsheet_formats::FormatError,
    },
    #[error("definition {path}: {message}")]
    Definition { path: String, message: String },
}

pub type Result<T, E = GameDataError> = std::result::Result<T, E>;
