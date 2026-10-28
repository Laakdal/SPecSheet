use std::path::PathBuf;

/// Errors produced while reading an archive.
#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("I/O error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("not a HashFS archive (missing `SCS#` magic)")]
    NotHashFs,
    #[error("unsupported HashFS version {0}")]
    UnsupportedVersion(u16),
    #[error("unsupported HashFS hash method `{0}`")]
    UnsupportedHashMethod(String),
    #[error("unsupported non-zero HashFS salt {0}")]
    UnsupportedSalt(u16),
    #[error("unsupported compression code {0}")]
    UnsupportedCompression(u8),
    #[error("not a ZIP archive (no end-of-central-directory record)")]
    NotZip,
    #[error("`{path}` uses unsupported ZIP compression method {method}")]
    UnsupportedZipMethod { path: String, method: u16 },
    #[error("corrupt archive {path}: {reason}")]
    Corrupt { path: PathBuf, reason: String },
    #[error("`{0}` not found")]
    NotFound(String),
    #[error("`{path}` is not a {expected}")]
    WrongEntryKind {
        path: String,
        expected: &'static str,
    },
}

pub type Result<T, E = ArchiveError> = std::result::Result<T, E>;
