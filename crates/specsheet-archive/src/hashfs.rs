//! HashFS version 2 container (`SCS#` magic), used by every `.scs` archive
//! shipped with ETS2 1.50+. Layout evidence: `docs/formats/hashfs.md`.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use flate2::read::ZlibDecoder;

use crate::city::city_hash_64;
use crate::error::{ArchiveError, Result};
use crate::gdeflate;
use crate::read_at::read_exact_at;
use crate::texture::{ImageMeta, PackedTexture, SampleMeta};

const MAGIC: &[u8; 4] = b"SCS#";
const HASH_METHOD_CITY: &[u8; 4] = b"CITY";
const HEADER_LEN: usize = 0x2c;
const ENTRY_LEN: usize = 16;
/// Stored blob offsets are expressed in 16-byte blocks.
const BLOCK_SIZE: u64 = 16;
/// Upper bound for any single table or blob we are willing to inflate. The
/// largest observed files are a few hundred MiB; this guards against hostile
/// size fields making us allocate unbounded memory.
const MAX_INFLATED_SIZE: u64 = 1 << 31;
const SIZE_MASK: u32 = 0x0fff_ffff;
/// Bit 28 of a blob's stored-size word marks it as compressed.
const COMPRESSED_BIT: u32 = 1 << 28;

/// Metadata record types observed in v2 archives.
mod record {
    pub const IMAGE: u8 = 0x01;
    pub const SAMPLE: u8 = 0x02;
    /// Secondary records of unknown purpose that follow a plain record and
    /// do not affect how content is read: 5 and 6 on some `.pmg`/`.pmd`
    /// entries, 7 (two words, e.g. `0x408, 0`) on 8,970 `effect.scs` shader
    /// entries.
    pub const AUX_5: u8 = 0x05;
    pub const AUX_6: u8 = 0x06;
    pub const AUX_7: u8 = 0x07;
    pub const PLAIN: u8 = 0x80;
    pub const DIRECTORY: u8 = 0x81;
    pub const MIP_TAIL: u8 = 0x84;
}

/// Location of one stored byte range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Blob {
    pub offset: u64,
    pub stored_size: u32,
    pub size: u32,
    /// Compressed with zlib (plain files) or GDeflate (packed textures).
    pub compressed: bool,
}

/// What an entry stores.
#[derive(Clone, Debug, PartialEq)]
pub enum Content {
    /// An ordinary file, or the encoded listing of a directory.
    Stored(Blob),
    /// A texture: HashFS v2 merges `.tobj` + `.dds` into one packed entry
    /// keyed by the `.tobj` path.
    Texture {
        image: ImageMeta,
        sample: SampleMeta,
        blob: Blob,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Directory,
}

/// One indexed entry of an archive.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub hash: u64,
    pub kind: EntryKind,
    pub content: Content,
}

/// Names contained in one directory listing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DirListing {
    pub files: Vec<String>,
    pub directories: Vec<String>,
}

/// An opened, indexed, read-only HashFS v2 archive.
#[derive(Debug)]
pub struct Archive {
    path: PathBuf,
    file: File,
    file_len: u64,
    entries: HashMap<u64, Entry>,
    skipped: Vec<SkippedEntry>,
}

/// An entry that could not be indexed (unknown record layout). Kept so
/// callers can report it instead of the whole archive failing to open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkippedEntry {
    pub hash: u64,
    pub reason: String,
}

#[derive(Debug)]
struct Header {
    entry_count: u32,
    entry_table_stored: u32,
    meta_table_words: u32,
    meta_table_stored: u32,
    entry_table_offset: u64,
    meta_table_offset: u64,
}

impl Archive {
    /// Opens and indexes the archive at `path`. Nothing is ever written.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let io_error = |source| ArchiveError::Io {
            path: path.clone(),
            source,
        };
        let file = File::open(&path).map_err(io_error)?;
        let file_len = file.metadata().map_err(io_error)?.len();
        let mut archive = Self {
            path,
            file,
            file_len,
            entries: HashMap::new(),
            skipped: Vec::new(),
        };
        archive.index()?;
        Ok(archive)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Number of indexed entries (files and directories).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.values()
    }

    /// Entries that were present but could not be indexed.
    pub fn skipped_entries(&self) -> &[SkippedEntry] {
        &self.skipped
    }

    /// Looks an entry up by its game path (e.g. `def/vehicle/truck`).
    pub fn entry(&self, path: &str) -> Option<&Entry> {
        self.entries.get(&hash_path(path))
    }

    /// Reads and decompresses a plain file.
    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        match &self.require(path)?.content {
            Content::Stored(blob) => self.read_stored(blob),
            Content::Texture { .. } => Err(wrong_kind(path, "plain file")),
        }
    }

    /// Reads a packed texture entry (addressed by its `.tobj` path).
    pub fn read_texture(&self, path: &str) -> Result<PackedTexture> {
        let Content::Texture {
            image,
            sample,
            blob,
        } = &self.require(path)?.content
        else {
            return Err(wrong_kind(path, "packed texture"));
        };
        let raw = self.read_range(blob.offset, u64::from(blob.stored_size))?;
        let surfaces = if blob.compressed {
            gdeflate::decompress(&raw)
                .map_err(|reason| self.corrupt(format!("texture `{path}`: {reason}")))?
        } else {
            raw
        };
        Ok(PackedTexture {
            image: *image,
            sample: *sample,
            surfaces,
        })
    }

    /// Decodes the listing of a directory entry.
    pub fn list_dir(&self, path: &str) -> Result<DirListing> {
        let entry = self.require(path)?;
        let (EntryKind::Directory, Content::Stored(blob)) = (entry.kind, &entry.content) else {
            return Err(wrong_kind(path, "directory"));
        };
        let bytes = self.read_stored(blob)?;
        parse_dir_listing(&bytes).ok_or_else(|| self.corrupt(format!("bad listing for `{path}`")))
    }

    fn require(&self, path: &str) -> Result<&Entry> {
        self.entry(path)
            .ok_or_else(|| ArchiveError::NotFound(path.to_owned()))
    }

    fn read_stored(&self, blob: &Blob) -> Result<Vec<u8>> {
        let stored = self.read_range(blob.offset, u64::from(blob.stored_size))?;
        if !blob.compressed {
            return Ok(stored);
        }
        inflate(&stored, u64::from(blob.size))
            .map_err(|reason| self.corrupt(format!("blob at {:#x}: {reason}", blob.offset)))
    }

    fn index(&mut self) -> Result<()> {
        let header = self.read_header()?;
        let entry_bytes = self.read_table(
            header.entry_table_offset,
            header.entry_table_stored,
            u64::from(header.entry_count) * ENTRY_LEN as u64,
        )?;
        let meta_bytes = self.read_table(
            header.meta_table_offset,
            header.meta_table_stored,
            u64::from(header.meta_table_words) * 4,
        )?;
        let meta = MetaTable { bytes: &meta_bytes };

        self.entries.reserve(header.entry_count as usize);
        for raw in entry_bytes.chunks_exact(ENTRY_LEN) {
            let hash = u64_le(raw, 0);
            let meta_index = u32_le(raw, 8);
            let meta_count = u16_le(raw, 12);
            match self.parse_entry(&meta, meta_index, meta_count) {
                Ok((kind, content)) => {
                    self.entries.insert(
                        hash,
                        Entry {
                            hash,
                            kind,
                            content,
                        },
                    );
                }
                Err(reason) => self.skipped.push(SkippedEntry { hash, reason }),
            }
        }
        Ok(())
    }

    fn read_header(&self) -> Result<Header> {
        let raw = self.read_range(0, HEADER_LEN as u64)?;
        if &raw[0..4] != MAGIC {
            return Err(ArchiveError::NotHashFs);
        }
        let version = u16_le(&raw, 4);
        if version != 2 {
            return Err(ArchiveError::UnsupportedVersion(version));
        }
        let salt = u16_le(&raw, 6);
        if salt != 0 {
            // Never observed in shipped archives; how a salt alters path
            // hashing is unverified, so refuse rather than mis-resolve paths.
            return Err(ArchiveError::UnsupportedSalt(salt));
        }
        if &raw[8..12] != HASH_METHOD_CITY {
            return Err(ArchiveError::UnsupportedHashMethod(
                String::from_utf8_lossy(&raw[8..12]).into_owned(),
            ));
        }
        Ok(Header {
            entry_count: u32_le(&raw, 12),
            entry_table_stored: u32_le(&raw, 16),
            meta_table_words: u32_le(&raw, 20),
            meta_table_stored: u32_le(&raw, 24),
            entry_table_offset: u64_le(&raw, 28),
            meta_table_offset: u64_le(&raw, 36),
        })
    }

    /// Reads a zlib-compressed index table.
    fn read_table(&self, offset: u64, stored: u32, expected: u64) -> Result<Vec<u8>> {
        let compressed = self.read_range(offset, u64::from(stored))?;
        inflate(&compressed, expected)
            .map_err(|reason| self.corrupt(format!("index table at {offset:#x}: {reason}")))
    }

    /// Interprets the metadata records of one entry. The first record is the
    /// primary one and decides what the entry is.
    fn parse_entry(
        &self,
        meta: &MetaTable<'_>,
        meta_index: u32,
        meta_count: u16,
    ) -> std::result::Result<(EntryKind, Content), String> {
        let mut image = None;
        let mut sample = None;
        let mut blob = None;
        let mut kind = EntryKind::File;

        for i in 0..u32::from(meta_count) {
            let [header] = meta.words::<1>(meta_index + i)?;
            let (record_type, data_index) = ((header >> 24) as u8, header & 0x00ff_ffff);
            match record_type {
                record::IMAGE => image = Some(ImageMeta::decode(meta.words::<2>(data_index)?)),
                record::SAMPLE => {
                    let [word] = meta.words::<1>(data_index)?;
                    sample = Some(SampleMeta::decode(word));
                }
                record::PLAIN | record::MIP_TAIL => {
                    blob = Some(self.decode_blob(meta.words::<4>(data_index)?)?);
                }
                record::DIRECTORY => {
                    kind = EntryKind::Directory;
                    blob = Some(self.decode_blob(meta.words::<4>(data_index)?)?);
                }
                record::AUX_5 | record::AUX_6 | record::AUX_7 => {}
                other => return Err(format!("unsupported metadata record type {other:#04x}")),
            }
        }

        let blob = blob.ok_or("entry has no data record")?;
        let content = match (image, sample) {
            (Some(image), Some(sample)) => Content::Texture {
                image,
                sample,
                blob,
            },
            (None, None) => Content::Stored(blob),
            _ => return Err("image record without matching sample record".to_owned()),
        };
        Ok((kind, content))
    }

    fn decode_blob(&self, words: [u32; 4]) -> std::result::Result<Blob, String> {
        let stored_size = words[0] & SIZE_MASK;
        let offset = u64::from(words[3]) * BLOCK_SIZE;
        if offset + u64::from(stored_size) > self.file_len {
            return Err(format!(
                "blob {offset:#x}+{stored_size} exceeds archive length"
            ));
        }
        Ok(Blob {
            offset,
            stored_size,
            size: words[1] & SIZE_MASK,
            compressed: words[0] & COMPRESSED_BIT != 0,
        })
    }

    fn read_range(&self, offset: u64, len: u64) -> Result<Vec<u8>> {
        if offset
            .checked_add(len)
            .is_none_or(|end| end > self.file_len)
        {
            return Err(self.corrupt(format!("range {offset:#x}+{len} exceeds archive length")));
        }
        let len = usize::try_from(len).map_err(|_| self.corrupt("range too large".to_owned()))?;
        let mut buf = vec![0; len];
        read_exact_at(&self.file, &mut buf, offset).map_err(|source| ArchiveError::Io {
            path: self.path.clone(),
            source,
        })?;
        Ok(buf)
    }

    fn corrupt(&self, reason: String) -> ArchiveError {
        ArchiveError::Corrupt {
            path: self.path.clone(),
            reason,
        }
    }
}

/// Hashes a game path the way HashFS keys it: no leading slash.
pub fn hash_path(path: &str) -> u64 {
    city_hash_64(path.trim_start_matches('/').as_bytes())
}

fn wrong_kind(path: &str, expected: &'static str) -> ArchiveError {
    ArchiveError::WrongEntryKind {
        path: path.to_owned(),
        expected,
    }
}

/// The metadata table: a flat array of little-endian `u32` words. Each entry
/// owns `meta_count` record headers (`type << 24 | data_word_index`).
struct MetaTable<'a> {
    bytes: &'a [u8],
}

impl MetaTable<'_> {
    fn words<const N: usize>(&self, index: u32) -> std::result::Result<[u32; N], String> {
        let start = index as usize * 4;
        let raw = self
            .bytes
            .get(start..start + N * 4)
            .ok_or_else(|| format!("metadata words {index}+{N} out of range"))?;
        let mut words = [0u32; N];
        for (word, chunk) in words.iter_mut().zip(raw.chunks_exact(4)) {
            *word = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        Ok(words)
    }
}

/// v2 directory listing: `u32 count`, `count` name lengths (`u8`), then the
/// concatenated names. Sub-directories are prefixed with `/`.
fn parse_dir_listing(bytes: &[u8]) -> Option<DirListing> {
    let count = u32::from_le_bytes(bytes.get(0..4)?.try_into().ok()?) as usize;
    let lengths = bytes.get(4..4usize.checked_add(count)?)?;
    let mut cursor = 4 + count;
    let mut listing = DirListing::default();
    for &len in lengths {
        let name = bytes.get(cursor..cursor + usize::from(len))?;
        cursor += usize::from(len);
        let name = String::from_utf8_lossy(name);
        match name.strip_prefix('/') {
            Some(dir) => listing.directories.push(dir.to_owned()),
            None => listing.files.push(name.into_owned()),
        }
    }
    Some(listing)
}

pub(crate) fn inflate(compressed: &[u8], expected: u64) -> std::result::Result<Vec<u8>, String> {
    if expected > MAX_INFLATED_SIZE {
        return Err(format!("declared size {expected} exceeds limit"));
    }
    let mut out = Vec::with_capacity(expected as usize);
    ZlibDecoder::new(compressed)
        .take(expected + 1)
        .read_to_end(&mut out)
        .map_err(|e| format!("zlib: {e}"))?;
    if out.len() as u64 != expected {
        return Err(format!(
            "inflated to {} bytes, expected {expected}",
            out.len()
        ));
    }
    Ok(out)
}

fn u16_le(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn u32_le(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn u64_le(bytes: &[u8], at: usize) -> u64 {
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&bytes[at..at + 8]);
    u64::from_le_bytes(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_directory_listing_with_files_and_subdirectories() {
        let mut bytes = 3u32.to_le_bytes().to_vec();
        bytes.extend([4, 3, 11]);
        bytes.extend(b"/def/uiversion.sii");
        let listing = parse_dir_listing(&bytes).expect("valid listing");
        assert_eq!(listing.directories, ["def", "ui"]);
        assert_eq!(listing.files, ["version.sii"]);
    }

    #[test]
    fn rejects_truncated_directory_listing() {
        let mut bytes = 2u32.to_le_bytes().to_vec();
        bytes.extend([4, 9]);
        bytes.extend(b"/def/ui");
        assert_eq!(parse_dir_listing(&bytes), None);
    }

    #[test]
    fn inflate_rejects_size_mismatch() {
        use flate2::{write::ZlibEncoder, Compression};
        use std::io::Write;
        let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
        enc.write_all(b"hello world").expect("write");
        let compressed = enc.finish().expect("finish");
        assert_eq!(inflate(&compressed, 11).expect("ok"), b"hello world");
        assert!(inflate(&compressed, 5).is_err());
        assert!(inflate(&compressed, 20).is_err());
    }
}
