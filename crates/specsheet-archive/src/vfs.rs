//! A layered, read-only view over several mounted archives, mirroring the
//! game's file system: archives mounted later override files of the same
//! path, and directory listings are merged across all archives.

use std::collections::BTreeSet;

use crate::error::{ArchiveError, Result};
use crate::hashfs::{Archive, DirListing, EntryKind};
use crate::texture::PackedTexture;

/// Archives in mount order (index 0 mounted first, lowest priority).
#[derive(Debug, Default)]
pub struct Vfs {
    archives: Vec<Archive>,
}

impl Vfs {
    /// An empty file system.
    pub fn new() -> Self {
        Self::default()
    }

    /// Mounts `archive` on top of everything mounted so far.
    pub fn mount(&mut self, archive: Archive) {
        self.archives.push(archive);
    }

    /// The mounted archives, first mounted first.
    pub fn archives(&self) -> &[Archive] {
        &self.archives
    }

    /// The highest-priority archive containing `path`.
    pub fn locate(&self, path: &str) -> Option<&Archive> {
        self.archives.iter().rev().find(|archive| {
            archive
                .entry(path)
                .is_some_and(|e| e.kind == EntryKind::File)
        })
    }

    /// Whether any archive holds the file `path`.
    pub fn exists(&self, path: &str) -> bool {
        self.locate(path).is_some()
    }

    /// Reads a plain file from the highest-priority archive containing it.
    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        self.locate(path)
            .ok_or_else(|| ArchiveError::NotFound(path.to_owned()))?
            .read(path)
    }

    /// Reads a packed texture (addressed by its `.tobj` path).
    pub fn read_texture(&self, path: &str) -> Result<PackedTexture> {
        self.locate(path)
            .ok_or_else(|| ArchiveError::NotFound(path.to_owned()))?
            .read_texture(path)
    }

    /// Merges the listing of `path` from every archive that has it. Names
    /// are sorted and de-duplicated.
    pub fn list_dir(&self, path: &str) -> Result<DirListing> {
        let mut files = BTreeSet::new();
        let mut directories = BTreeSet::new();
        let mut found = false;
        for archive in &self.archives {
            if !archive
                .entry(path)
                .is_some_and(|e| e.kind == EntryKind::Directory)
            {
                continue;
            }
            found = true;
            let listing = archive.list_dir(path)?;
            files.extend(listing.files);
            directories.extend(listing.directories);
        }
        if !found {
            return Err(ArchiveError::NotFound(path.to_owned()));
        }
        Ok(DirListing {
            files: files.into_iter().collect(),
            directories: directories.into_iter().collect(),
        })
    }
}
