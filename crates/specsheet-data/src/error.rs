use std::path::PathBuf;

/// Errors raised while loading game data.
#[derive(Debug, thiserror::Error)]
pub enum GameDataError {
    /// Discovery found no game directory.
    #[error("no ETS2 installation found; set ETS2_DIR to the game directory")]
    InstallNotFound,
    #[error("{path} is not an ETS2 installation: {reason}")]
    /// The directory is not a usable install.
    NotAnInstall {
        /// The directory checked.
        path: PathBuf,
        /// What is missing.
        reason: String,
    },
    /// The operating system failed to read a file.
    #[error("I/O error on {path}: {source}")]
    Io {
        /// The file involved.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
    /// An archive could not be read.
    #[error(transparent)]
    Archive(#[from] specsheet_archive::ArchiveError),
    /// A definition file does not parse.
    #[error(transparent)]
    Sii(#[from] specsheet_formats::sii::SiiError),
    #[error("{path}: {source}")]
    /// A binary file does not parse.
    Format {
        /// The file.
        path: String,
        /// The parser's error.
        #[source]
        source: specsheet_formats::FormatError,
    },
    #[error("definition {path}: {message}")]
    /// A definition is missing something the loader needs.
    Definition {
        /// The definition file.
        path: String,
        /// What is missing.
        message: String,
    },
}

/// A result with [`GameDataError`] as the default error.
pub type Result<T, E = GameDataError> = std::result::Result<T, E>;
