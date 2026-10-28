use std::path::PathBuf;

/// Errors produced while reading an archive or writing a package.
#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    /// The operating system failed to read or write a file.
    #[error("I/O error on {path}: {source}")]
    Io {
        /// The file involved.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The file does not start with the HashFS magic.
    #[error("not a HashFS archive (missing `SCS#` magic)")]
    NotHashFs,
    /// A HashFS version this reader does not handle.
    #[error("unsupported HashFS version {0}")]
    UnsupportedVersion(u16),
    /// A path hash method other than CityHash64.
    #[error("unsupported HashFS hash method `{0}`")]
    UnsupportedHashMethod(String),
    /// A salted HashFS v2 archive.
    #[error("unsupported non-zero HashFS salt {0}")]
    UnsupportedSalt(u16),
    /// An entry compression code this reader does not handle.
    #[error("unsupported compression code {0}")]
    UnsupportedCompression(u8),
    /// The file has no ZIP end-of-central-directory record.
    #[error("not a ZIP archive (no end-of-central-directory record)")]
    NotZip,
    /// A ZIP entry compressed with neither stored nor deflate.
    #[error("`{path}` uses unsupported ZIP compression method {method}")]
    UnsupportedZipMethod {
        /// The entry's path.
        path: String,
        /// The ZIP method code.
        method: u16,
    },
    /// A path given to the package writer is not allowed.
    #[error("package path `{path}` is {reason}")]
    InvalidPackagePath {
        /// The rejected path.
        path: String,
        /// Why it was rejected.
        reason: &'static str,
    },
    /// The package writer was given the same path twice.
    #[error("package path `{0}` is added twice")]
    DuplicatePackagePath(String),
    /// The package exceeds the plain (non-ZIP64) ZIP limits.
    #[error("package would need ZIP64: {0}")]
    PackageTooLarge(String),
    /// The package writer never replaces an existing file.
    #[error("a package named like `{}` already exists", .0.display())]
    PackageExists(PathBuf),
    /// The archive's structure is inconsistent.
    #[error("corrupt archive {path}: {reason}")]
    Corrupt {
        /// The archive file.
        path: PathBuf,
        /// What is wrong.
        reason: String,
    },
    /// No entry has this path.
    #[error("`{0}` not found")]
    NotFound(String),
    /// The entry exists but is the wrong kind (a file where a directory was
    /// asked for, or the reverse).
    #[error("`{path}` is not a {expected}")]
    WrongEntryKind {
        /// The entry's path.
        path: String,
        /// The kind that was asked for.
        expected: &'static str,
    },
}

/// A result with [`ArchiveError`] as the default error.
pub type Result<T, E = ArchiveError> = std::result::Result<T, E>;
