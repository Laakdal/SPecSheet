//! Typed access to game files on top of the layered archive file system.

use specsheet_archive::{DirListing, Vfs};
use specsheet_formats::sii::{self, SiiDocument};

use crate::error::Result;

/// The mounted game file system. Paths are absolute game paths
/// (`/def/vehicle/...`); a leading slash is optional.
#[derive(Debug)]
pub struct GameFs {
    vfs: Vfs,
}

impl GameFs {
    /// The game's file system over a mounted `vfs`.
    pub fn new(vfs: Vfs) -> Self {
        Self { vfs }
    }

    /// The mounted archives.
    pub fn vfs(&self) -> &Vfs {
        &self.vfs
    }

    /// Whether the file `path` exists (leading slash optional).
    pub fn exists(&self, path: &str) -> bool {
        self.vfs.exists(normalize(path))
    }

    /// The bytes of the file `path`.
    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        Ok(self.vfs.read(normalize(path))?)
    }

    /// The files and subdirectories of `path`, across every archive.
    pub fn list_dir(&self, path: &str) -> Result<DirListing> {
        Ok(self.vfs.list_dir(normalize(path).trim_end_matches('/'))?)
    }

    /// Reads and parses a text SII file, expanding `@include` directives.
    pub fn read_sii(&self, path: &str) -> Result<SiiDocument> {
        let absolute = absolute(path);
        let bytes = self.read(&absolute)?;
        let loader = |include: &str| self.read(include).map_err(|e| e.to_string());
        Ok(sii::parse_with_includes(&absolute, &bytes, &loader)?)
    }
}

/// Archive lookups use paths without the leading slash.
fn normalize(path: &str) -> &str {
    path.trim_start_matches('/')
}

fn absolute(path: &str) -> String {
    format!("/{}", normalize(path))
}
