//! Typed access to game files on top of the layered archive file system.

use scs_archive::{DirListing, Vfs};
use scs_formats::sii::{self, SiiDocument};

use crate::error::Result;

/// The mounted game file system. Paths are absolute game paths
/// (`/def/vehicle/...`); a leading slash is optional.
#[derive(Debug)]
pub struct GameFs {
    vfs: Vfs,
}

impl GameFs {
    pub fn new(vfs: Vfs) -> Self {
        Self { vfs }
    }

    pub fn vfs(&self) -> &Vfs {
        &self.vfs
    }

    pub fn exists(&self, path: &str) -> bool {
        self.vfs.exists(normalize(path))
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        Ok(self.vfs.read(normalize(path))?)
    }

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
