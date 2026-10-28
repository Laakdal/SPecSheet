//! Read-only access to SCS Software HashFS (`.scs`) archives and mod
//! packages, and a writer for new ZIP mod packages (D18).
//!
//! The crate never writes to existing archives; [`ZipWriter`] only builds new
//! packages in memory. All size and offset fields read from
//! disk are validated before use, since game files are treated as untrusted.

#![warn(missing_docs)]

mod city;
mod error;
mod gdeflate;
mod hashfs;
mod hashfs_v1;
mod read_at;
mod texture;
mod vfs;
mod zip;
mod zip_write;

pub use city::city_hash_64;
pub use error::{ArchiveError, Result};
pub use hashfs::{hash_path, Archive, Blob, Content, DirListing, Entry, EntryKind, SkippedEntry};
pub use hashfs_v1::{EntryV1, HashFsV1Archive};
pub use texture::{
    pixel_layout, AddressMode, ImageMeta, PackedTexture, PixelLayout, SampleMeta, Surface,
};
pub use vfs::Vfs;
pub use zip::ZipArchive;
pub use zip_write::{write_new_package, ZipWriter};
