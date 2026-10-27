//! Read-only access to SCS Software HashFS (`.scs`) archives.
//!
//! The crate never writes to archives. All size and offset fields read from
//! disk are validated before use, since game files are treated as untrusted.

mod city;
mod error;
mod gdeflate;
mod hashfs;
mod read_at;
mod texture;
mod vfs;

pub use city::city_hash_64;
pub use error::{ArchiveError, Result};
pub use hashfs::{hash_path, Archive, Blob, Content, DirListing, Entry, EntryKind, SkippedEntry};
pub use texture::{
    pixel_layout, AddressMode, ImageMeta, PackedTexture, PixelLayout, SampleMeta, Surface,
};
pub use vfs::Vfs;
