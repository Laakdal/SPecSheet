//! Read-only HashFS v2 (`.scs`) archives: the header, the entry and metadata
//! tables, plain files, packed textures and directory listings.
//! Format: `docs/formats/hashfs.md`, derived from our own hex dumps
//! (`research/evidence/hashfs-layout-derivation.txt`, cited below as "D:"
//! plus a section number).

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::error::{ArchiveError, Result};
use crate::read_at::read_exact_at;
use crate::texture::{ImageMeta, PackedTexture, SampleMeta};

/// Where one stored byte range sits in the archive file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Blob {
    /// Absolute byte offset in the archive file.
    pub offset: u64,
    /// Bytes stored in the archive.
    pub stored_size: u32,
    /// Bytes after decompression (equal to `stored_size` when stored raw).
    pub size: u32,
    /// Compressed: zlib for plain files and listings, GDeflate for packed
    /// texture surfaces.
    pub compressed: bool,
}

/// What an entry holds.
#[derive(Clone, Debug, PartialEq)]
pub enum Content {
    /// A plain file, or the encoded listing of a directory.
    Stored(Blob),
    /// A packed texture: the `.tobj` and its pixel data stored as one entry,
    /// keyed by the `.tobj` path. `image` and `sample` come from the entry's
    /// metadata records (decoded with `ImageMeta::decode` / `SampleMeta::decode`).
    Texture {
        /// Image layout: size, format, mipmaps, faces.
        image: ImageMeta,
        /// Sampler settings of the `.tobj`.
        sample: SampleMeta,
        /// The packed pixel data.
        blob: Blob,
    },
}

/// Whether an entry is a file or a directory listing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    /// A file.
    File,
    /// A directory; its content is the encoded listing.
    Directory,
}

/// One indexed entry.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// `hash_path` of the entry's game path.
    pub hash: u64,
    /// File or directory.
    pub kind: EntryKind,
    /// Where its bytes are.
    pub content: Content,
}

/// The decoded listing of a directory entry. Names are relative, and
/// directory names carry no marker (no leading `/`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DirListing {
    /// File names.
    pub files: Vec<String>,
    /// Subdirectory names.
    pub directories: Vec<String>,
}

/// An entry the archive lists but that could not be indexed (e.g. an unknown
/// metadata record type). Recorded instead of failing the whole archive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkippedEntry {
    /// The entry's path hash.
    pub hash: u64,
    /// Why it was skipped.
    pub reason: String,
}

// Header (D:1). All fields little-endian.
const MAGIC: &[u8; 4] = b"SCS#";
const HASH_METHOD: &[u8; 4] = b"CITY";
const SUPPORTED_VERSION: u16 = 2;
/// Bytes holding header fields; everything after is not needed here.
const HEADER_LEN: usize = 0x34;

// Entry table (D:2): 16-byte records.
const ENTRY_LEN: usize = 16;

// Metadata records (D:3): one header word `type << 24 | data word index`.
const RECORD_IMAGE: u8 = 0x01;
const RECORD_SAMPLE: u8 = 0x02;
const RECORD_UNKNOWN_05: u8 = 0x05;
const RECORD_UNKNOWN_06: u8 = 0x06;
const RECORD_UNKNOWN_07: u8 = 0x07;
const RECORD_PLAIN: u8 = 0x80;
const RECORD_DIRECTORY: u8 = 0x81;
const RECORD_MIP_TAIL: u8 = 0x84;

// Blob words (D:4).
const SIZE_MASK: u32 = 0x0fff_ffff;
const FLAG_COMPRESSED: u32 = 1 << 28;
/// Set on every GDeflate-compressed texture blob, never elsewhere.
const FLAG_GDEFLATE: u32 = 1 << 29;
const OFFSET_UNIT: u64 = 16;

/// Upper bound for an inflated table. The largest 1.60 tables are a few
/// MiB (D:2); this rejects absurd header counts before inflating.
const MAX_TABLE_BYTES: u64 = 256 << 20;
/// Deflate cannot expand data by more than about 1032:1. A recorded size
/// beyond this is rejected before any allocation.
const MAX_DEFLATE_RATIO: u64 = 1032;
/// GDeflate tile size and stream header length (D:4, `crate::gdeflate`).
const GDEFLATE_TILE: u64 = 64 * 1024;
const GDEFLATE_HEADER_LEN: usize = 8;

/// An opened, indexed archive. Must be `Send + Sync`: several threads read
/// through one `Archive` (use positional reads, e.g. `crate::read_at`, not a
/// shared cursor). Nothing is ever written.
#[derive(Debug)]
pub struct Archive {
    path: PathBuf,
    file: File,
    file_len: u64,
    /// Sorted by hash, unique.
    entries: Vec<Entry>,
    skipped: Vec<SkippedEntry>,
}

impl Archive {
    /// Opens and indexes the archive at `path`.
    ///
    /// Errors: `Io` (open or read failure), `NotHashFs` (wrong magic),
    /// `UnsupportedVersion`, `UnsupportedHashMethod`, `UnsupportedSalt`,
    /// `UnsupportedCompression` (tables), `Corrupt` (out-of-range or
    /// inconsistent tables). An entry whose metadata can't be interpreted is
    /// recorded in `skipped_entries`; it doesn't fail the archive. Treat
    /// the file as hostile: bound every allocation by the file size, and
    /// never panic.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let io = |source| ArchiveError::Io {
            path: path.clone(),
            source,
        };
        let file = File::open(&path).map_err(io)?;
        let file_len = file.metadata().map_err(io)?.len();
        let mut archive = Self {
            path,
            file,
            file_len,
            entries: Vec::new(),
            skipped: Vec::new(),
        };
        archive.index()?;
        Ok(archive)
    }

    /// The path the archive was opened from.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Number of indexed entries (files and directories).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the archive indexes no entry.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// All indexed entries, in no particular order.
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter()
    }

    /// Entries that were listed but could not be indexed.
    pub fn skipped_entries(&self) -> &[SkippedEntry] {
        &self.skipped
    }

    /// The entry for a game path such as `def/vehicle/truck`, with or without
    /// a leading `/` (looked up by `hash_path`). The root directory is `""`.
    pub fn entry(&self, path: &str) -> Option<&Entry> {
        let hash = hash_path(path);
        self.entries
            .binary_search_by_key(&hash, |e| e.hash)
            .ok()
            .and_then(|i| self.entries.get(i))
    }

    /// Reads a plain file, decompressed. `NotFound` if absent, and
    /// `WrongEntryKind { expected: "plain file" }` for a packed texture.
    /// `Corrupt` if the data is out of range or doesn't decompress to its
    /// recorded size.
    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        let entry = self.find(path)?;
        match (&entry.content, entry.kind) {
            (Content::Stored(blob), EntryKind::File) => self.read_zlib_blob(blob),
            _ => Err(wrong_kind(path, "plain file")),
        }
    }

    /// Reads a packed texture by its `.tobj` path: its metadata plus the
    /// surface bytes, GDeflate-decompressed when compressed (`crate::gdeflate`).
    /// `NotFound` if absent, and `WrongEntryKind { expected: "packed texture" }`
    /// for anything else.
    pub fn read_texture(&self, path: &str) -> Result<PackedTexture> {
        let entry = self.find(path)?;
        let Content::Texture {
            image,
            sample,
            blob,
        } = &entry.content
        else {
            return Err(wrong_kind(path, "packed texture"));
        };
        let stored = self.read_stored(blob)?;
        let surfaces = if blob.compressed {
            self.check_gdeflate_size(&stored, blob.size)?;
            crate::gdeflate::decompress(&stored)
                .map_err(|e| self.corrupt(format!("{path}: {e}")))?
        } else {
            stored
        };
        self.check_size(surfaces.len(), blob.size)?;
        Ok(PackedTexture {
            image: *image,
            sample: *sample,
            surfaces,
        })
    }

    /// Decodes a directory's listing. `NotFound` if absent,
    /// `WrongEntryKind { expected: "directory" }` if not a directory, and
    /// `Corrupt` if the listing doesn't decode.
    pub fn list_dir(&self, path: &str) -> Result<DirListing> {
        let entry = self.find(path)?;
        match (&entry.content, entry.kind) {
            (Content::Stored(blob), EntryKind::Directory) => {
                let bytes = self.read_zlib_blob(blob)?;
                decode_listing(&bytes).map_err(|reason| self.corrupt(format!("{path}: {reason}")))
            }
            _ => Err(wrong_kind(path, "directory")),
        }
    }

    fn find(&self, path: &str) -> Result<&Entry> {
        self.entry(path)
            .ok_or_else(|| ArchiveError::NotFound(path.to_owned()))
    }

    fn corrupt(&self, reason: impl Into<String>) -> ArchiveError {
        ArchiveError::Corrupt {
            path: self.path.clone(),
            reason: reason.into(),
        }
    }

    /// Reads `len` bytes at `offset`, which must lie inside the file.
    fn read_range(&self, offset: u64, len: u64, what: &str) -> Result<Vec<u8>> {
        if offset
            .checked_add(len)
            .is_none_or(|end| end > self.file_len)
        {
            return Err(self.corrupt(format!(
                "{what} at {offset:#x}+{len} lies outside the {}-byte file",
                self.file_len
            )));
        }
        let len = usize::try_from(len).map_err(|_| self.corrupt(format!("{what} too large")))?;
        let mut buf = vec![0; len];
        read_exact_at(&self.file, &mut buf, offset).map_err(|source| ArchiveError::Io {
            path: self.path.clone(),
            source,
        })?;
        Ok(buf)
    }

    fn read_stored(&self, blob: &Blob) -> Result<Vec<u8>> {
        self.read_range(blob.offset, u64::from(blob.stored_size), "data")
    }

    /// A plain file or listing: zlib when compressed, else stored as is.
    fn read_zlib_blob(&self, blob: &Blob) -> Result<Vec<u8>> {
        let stored = self.read_stored(blob)?;
        let data = if blob.compressed {
            inflate_exact(&stored, u64::from(blob.size)).map_err(|e| self.corrupt(e))?
        } else {
            stored
        };
        self.check_size(data.len(), blob.size)?;
        Ok(data)
    }

    fn check_size(&self, actual: usize, recorded: u32) -> Result<()> {
        if u64::try_from(actual).ok() == Some(u64::from(recorded)) {
            Ok(())
        } else {
            Err(self.corrupt(format!(
                "data is {actual} bytes, but {recorded} are recorded"
            )))
        }
    }

    /// Checks the size a GDeflate stream header implies (D:4), and that the
    /// stream is large enough to hold it, before decompressing. A hostile
    /// tile count or size can't force a large allocation.
    fn check_gdeflate_size(&self, stream: &[u8], recorded: u32) -> Result<()> {
        if u64::from(recorded) > ratio_limit(stream.len()) {
            return Err(self.corrupt(format!(
                "{} stored bytes cannot decompress to {recorded}",
                stream.len()
            )));
        }
        let header = stream
            .get(..GDEFLATE_HEADER_LEN)
            .ok_or_else(|| self.corrupt("GDeflate stream shorter than its header"))?;
        let tiles = u64::from(le_u16(header, 2).unwrap_or(0));
        let last = u64::from((le_u32(header, 4).unwrap_or(0) >> 2) & 0x3_ffff);
        let implied = match tiles {
            0 => 0,
            _ => (tiles - 1) * GDEFLATE_TILE + if last == 0 { GDEFLATE_TILE } else { last },
        };
        if implied == u64::from(recorded) {
            Ok(())
        } else {
            Err(self.corrupt(format!(
                "GDeflate stream holds {implied} bytes, but {recorded} are recorded"
            )))
        }
    }

    /// Reads and checks the header, inflates both tables and indexes every
    /// entry.
    fn index(&mut self) -> Result<()> {
        let header_len = HEADER_LEN.min(usize::try_from(self.file_len).unwrap_or(HEADER_LEN));
        let header = self.read_range(0, header_len as u64, "header")?;
        if header.get(..4) != Some(MAGIC.as_slice()) {
            return Err(ArchiveError::NotHashFs);
        }
        let header = Header::parse(&header).ok_or_else(|| self.corrupt("truncated header"))?;
        if header.version != SUPPORTED_VERSION {
            return Err(ArchiveError::UnsupportedVersion(header.version));
        }
        if header.salt != 0 {
            return Err(ArchiveError::UnsupportedSalt(header.salt));
        }
        if &header.hash_method != HASH_METHOD {
            return Err(ArchiveError::UnsupportedHashMethod(
                String::from_utf8_lossy(&header.hash_method).into_owned(),
            ));
        }

        let entry_bytes = u64::from(header.entry_count) * ENTRY_LEN as u64;
        let entry_table = self.read_table(
            "entry table",
            header.entry_table_offset,
            header.entry_table_stored,
            entry_bytes,
        )?;
        let meta_bytes = u64::from(header.metadata_words) * 4;
        let meta_table = self.read_table(
            "metadata table",
            header.metadata_offset,
            header.metadata_stored,
            meta_bytes,
        )?;
        let meta: Vec<u32> = meta_table
            .chunks_exact(4)
            .map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
            .collect();

        let mut entries = Vec::with_capacity(entry_table.len() / ENTRY_LEN);
        let mut skipped = Vec::new();
        for record in entry_table.chunks_exact(ENTRY_LEN) {
            let raw = RawEntry::parse(record).ok_or_else(|| self.corrupt("short entry record"))?;
            let records = raw
                .records(&meta)
                .map_err(|reason| self.corrupt(format!("entry {:#018x}: {reason}", raw.hash)))?;
            match classify(raw.hash, &records) {
                Ok(entry) => entries.push(entry),
                Err(reason) => skipped.push(SkippedEntry {
                    hash: raw.hash,
                    reason,
                }),
            }
        }
        entries.sort_unstable_by_key(|e| e.hash);
        let all_hashes = entries
            .iter()
            .map(|e| e.hash)
            .chain(skipped.iter().map(|s| s.hash));
        let mut hashes: Vec<u64> = all_hashes.collect();
        hashes.sort_unstable();
        if let Some(pair) = hashes.windows(2).find(|w| w.first() == w.get(1)) {
            let hash = pair.first().copied().unwrap_or(0);
            return Err(self.corrupt(format!("hash {hash:#018x} is listed twice")));
        }
        self.entries = entries;
        self.skipped = skipped;
        Ok(())
    }

    /// Reads and inflates a zlib table that must inflate to exactly
    /// `expected` bytes.
    fn read_table(&self, what: &str, offset: u64, stored: u32, expected: u64) -> Result<Vec<u8>> {
        if expected > MAX_TABLE_BYTES {
            return Err(self.corrupt(format!("{what} of {expected} bytes is implausibly large")));
        }
        let raw = self.read_range(offset, u64::from(stored), what)?;
        // Every table in 1.60 is a zlib stream (D:2). The zlib header's
        // low nibble names the method; 8 is deflate.
        let method = raw.first().map_or(0, |b| b & 0x0f);
        if method != 8 {
            return Err(ArchiveError::UnsupportedCompression(method));
        }
        inflate_exact(&raw, expected).map_err(|e| self.corrupt(format!("{what}: {e}")))
    }
}

// Several threads read through one `Archive`.
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Archive>();
};

/// The key HashFS stores for a game path: CityHash64 v1.0.3
/// (`crate::city::city_hash_64`) of the path without a leading `/`.
pub fn hash_path(path: &str) -> u64 {
    let path = path.strip_prefix('/').unwrap_or(path);
    crate::city::city_hash_64(path.as_bytes())
}

fn wrong_kind(path: &str, expected: &'static str) -> ArchiveError {
    ArchiveError::WrongEntryKind {
        path: path.to_owned(),
        expected,
    }
}

fn le_u16(bytes: &[u8], at: usize) -> Option<u16> {
    let b = bytes.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([*b.first()?, *b.get(1)?]))
}

fn le_u32(bytes: &[u8], at: usize) -> Option<u32> {
    let b: [u8; 4] = bytes.get(at..at.checked_add(4)?)?.try_into().ok()?;
    Some(u32::from_le_bytes(b))
}

fn le_u64(bytes: &[u8], at: usize) -> Option<u64> {
    let b: [u8; 8] = bytes.get(at..at.checked_add(8)?)?.try_into().ok()?;
    Some(u64::from_le_bytes(b))
}

/// The most bytes `stored` compressed bytes can expand to.
fn ratio_limit(stored: usize) -> u64 {
    (stored as u64)
        .saturating_mul(MAX_DEFLATE_RATIO)
        .saturating_add(1024)
}

/// Inflates a zlib stream that must produce exactly `expected` bytes. The
/// output only grows as data actually inflates, and stops one byte past
/// `expected`, so a lying size cannot force a large allocation.
fn inflate_exact(stored: &[u8], expected: u64) -> Result<Vec<u8>, String> {
    if expected > ratio_limit(stored.len()) {
        return Err(format!(
            "{} stored bytes cannot inflate to {expected}",
            stored.len()
        ));
    }
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(stored)
        .take(expected.saturating_add(1))
        .read_to_end(&mut out)
        .map_err(|e| format!("zlib: {e}"))?;
    if out.len() as u64 != expected {
        return Err(format!(
            "inflates to {}{} bytes, but {expected} are recorded",
            out.len(),
            if out.len() as u64 > expected { "+" } else { "" }
        ));
    }
    Ok(out)
}

/// The header fields (D:1).
struct Header {
    version: u16,
    salt: u16,
    hash_method: [u8; 4],
    entry_count: u32,
    entry_table_stored: u32,
    metadata_words: u32,
    metadata_stored: u32,
    entry_table_offset: u64,
    metadata_offset: u64,
}

impl Header {
    fn parse(bytes: &[u8]) -> Option<Self> {
        Some(Self {
            version: le_u16(bytes, 0x04)?,
            salt: le_u16(bytes, 0x06)?,
            hash_method: bytes.get(0x08..0x0c)?.try_into().ok()?,
            entry_count: le_u32(bytes, 0x0c)?,
            entry_table_stored: le_u32(bytes, 0x10)?,
            metadata_words: le_u32(bytes, 0x14)?,
            metadata_stored: le_u32(bytes, 0x18)?,
            entry_table_offset: le_u64(bytes, 0x1c)?,
            metadata_offset: le_u64(bytes, 0x24)?,
            // 0x2c: u64, 0x80 in every 1.60 archive; not needed (D:1).
        })
    }
}

/// One entry-table record (D:2).
struct RawEntry {
    hash: u64,
    metadata_index: u32,
    record_count: u16,
}

impl RawEntry {
    fn parse(bytes: &[u8]) -> Option<Self> {
        Some(Self {
            hash: le_u64(bytes, 0)?,
            metadata_index: le_u32(bytes, 8)?,
            record_count: le_u16(bytes, 12)?,
            // 14: u16 flags; bit 0 mirrors the directory record, the rest
            // is unknown (D:2). The records decide the kind.
        })
    }

    /// The entry's metadata records: `record_count` header words from
    /// `metadata_index`, each pointing at its payload (D:3).
    fn records<'m>(&self, meta: &'m [u32]) -> Result<Vec<Record<'m>>, String> {
        let start = usize::try_from(self.metadata_index).map_err(|e| e.to_string())?;
        let end = start
            .checked_add(usize::from(self.record_count))
            .filter(|&end| end <= meta.len())
            .ok_or_else(|| {
                format!(
                    "records {start}+{} lie outside the {}-word metadata table",
                    self.record_count,
                    meta.len()
                )
            })?;
        let headers = meta.get(start..end).unwrap_or_default();
        headers
            .iter()
            .map(|&word| {
                let kind = (word >> 24) as u8;
                let data = (word & 0x00ff_ffff) as usize;
                let payload = match payload_words(kind) {
                    Some(len) => data
                        .checked_add(len)
                        .and_then(|end| meta.get(data..end))
                        .ok_or_else(|| {
                            format!(
                                "record {kind:#04x} payload at word {data} lies outside the metadata table"
                            )
                        })?,
                    None => &[],
                };
                Ok(Record { kind, payload })
            })
            .collect()
    }
}

/// A metadata record: its type and payload words (empty for unknown types).
struct Record<'m> {
    kind: u8,
    payload: &'m [u32],
}

/// Payload length in words of each known record type (D:3).
fn payload_words(kind: u8) -> Option<usize> {
    match kind {
        RECORD_PLAIN | RECORD_DIRECTORY | RECORD_MIP_TAIL => Some(4),
        RECORD_IMAGE | RECORD_UNKNOWN_06 | RECORD_UNKNOWN_07 => Some(2),
        RECORD_SAMPLE => Some(1),
        RECORD_UNKNOWN_05 => Some(8),
        _ => None,
    }
}

/// Turns an entry's records into an `Entry`, or a reason to skip it.
///
/// Observed combinations (D:3): `0x80` (+ one of `0x05`/`0x06`/`0x07`),
/// `0x81`, and `0x01 + 0x02 + 0x84`. Records 0x05/0x06/0x07 don't change
/// how the content is read.
fn classify(hash: u64, records: &[Record]) -> Result<Entry, String> {
    let mut plain = None;
    let mut directory = None;
    let mut surface = None;
    let mut image = None;
    let mut sample = None;
    for record in records {
        let slot = match record.kind {
            RECORD_PLAIN => &mut plain,
            RECORD_DIRECTORY => &mut directory,
            RECORD_MIP_TAIL => &mut surface,
            RECORD_IMAGE => &mut image,
            RECORD_SAMPLE => &mut sample,
            RECORD_UNKNOWN_05 | RECORD_UNKNOWN_06 | RECORD_UNKNOWN_07 => continue,
            other => return Err(format!("unknown metadata record type {other:#04x}")),
        };
        if slot.replace(record.payload).is_some() {
            return Err(format!("duplicate metadata record {:#04x}", record.kind));
        }
    }
    let (kind, content) = match (plain, directory, surface, image, sample) {
        (Some(words), None, None, None, None) => {
            (EntryKind::File, Content::Stored(blob(words, false)?))
        }
        (None, Some(words), None, None, None) => {
            (EntryKind::Directory, Content::Stored(blob(words, false)?))
        }
        (None, None, Some(words), Some(image), Some(sample)) => {
            let image = match *image {
                [dims, flags] => ImageMeta::decode([dims, flags]),
                _ => return Err("short image record".to_owned()),
            };
            let sample = match *sample {
                [word] => SampleMeta::decode(word),
                _ => return Err("short sample record".to_owned()),
            };
            let blob = blob(words, true)?;
            (
                EntryKind::File,
                Content::Texture {
                    image,
                    sample,
                    blob,
                },
            )
        }
        _ => {
            let kinds: Vec<String> = records.iter().map(|r| format!("{:#04x}", r.kind)).collect();
            return Err(format!(
                "unexpected metadata record combination [{}]",
                kinds.join(", ")
            ));
        }
    };
    Ok(Entry {
        hash,
        kind,
        content,
    })
}

/// Decodes a 4-word blob location (D:4): stored size and flags, size, an
/// always-zero word, and the offset in 16-byte units.
fn blob(words: &[u32], texture: bool) -> Result<Blob, String> {
    let [stored, size, _always_zero, offset] = *words else {
        return Err("short blob record".to_owned());
    };
    // The stored-size word's top bits pick the codec. Plain files and
    // listings use 0 (raw) or bit 28 (zlib); texture surfaces 0 (raw) or
    // bits 28 + 29 (GDeflate). The size word's top bits (0 on plain blobs,
    // always 0x3 on texture blobs, raw or not) don't affect reading.
    let flags = stored & !SIZE_MASK;
    let compressed = flags & FLAG_COMPRESSED != 0;
    let known = if texture {
        flags == 0 || flags == FLAG_COMPRESSED | FLAG_GDEFLATE
    } else {
        flags == 0 || flags == FLAG_COMPRESSED
    };
    if !known {
        return Err(format!("unknown blob flags {:#x}", stored >> 28));
    }
    Ok(Blob {
        offset: u64::from(offset) * OFFSET_UNIT,
        stored_size: stored & SIZE_MASK,
        size: size & SIZE_MASK,
        compressed,
    })
}

/// Decodes a directory listing (D:5): `u32 count`, `count` name lengths
/// (`u8`), then the names back to back. A leading `/` marks a directory.
fn decode_listing(bytes: &[u8]) -> Result<DirListing, String> {
    let count = le_u32(bytes, 0).ok_or("listing shorter than its count")?;
    let count = usize::try_from(count).map_err(|e| e.to_string())?;
    let lengths = 4usize
        .checked_add(count)
        .and_then(|end| bytes.get(4..end))
        .ok_or_else(|| format!("listing too short for {count} name lengths"))?;
    let mut position = 4 + count;
    let mut listing = DirListing::default();
    for &length in lengths {
        let end = position + usize::from(length);
        let raw = bytes
            .get(position..end)
            .ok_or("listing names run past its end")?;
        position = end;
        let name = std::str::from_utf8(raw).map_err(|_| "listing name is not UTF-8")?;
        match name.strip_prefix('/') {
            Some(directory) => listing.directories.push(directory.to_owned()),
            None => listing.files.push(name.to_owned()),
        }
    }
    if position != bytes.len() {
        return Err(format!(
            "listing has {} trailing bytes",
            bytes.len() - position
        ));
    }
    Ok(listing)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    /// Builds small synthetic archives laid out like the game's (D:1-5).
    #[derive(Default)]
    struct Builder {
        /// File bytes from offset 0x1000 on.
        data: Vec<u8>,
        entries: Vec<(u64, u16, Vec<RawRecord>)>,
    }

    /// A record type and its payload words.
    type RawRecord = (u8, Vec<u32>);

    const DATA_START: usize = 0x1000;

    impl Builder {
        /// Appends bytes 16-aligned and returns their absolute offset in
        /// 16-byte units.
        fn put(&mut self, bytes: &[u8]) -> u32 {
            while !self.data.len().is_multiple_of(16) {
                self.data.push(0);
            }
            let offset = (DATA_START + self.data.len()) / 16;
            self.data.extend_from_slice(bytes);
            offset as u32
        }

        fn entry(&mut self, path: &str, flags: u16, records: Vec<(u8, Vec<u32>)>) {
            self.entries.push((hash_path(path), flags, records));
        }

        /// A plain file, zlib-compressed when `compress`.
        fn file(&mut self, path: &str, contents: &[u8], compress: bool) {
            let words = self.blob_words(contents, compress);
            self.entry(path, 0, vec![(RECORD_PLAIN, words.to_vec())]);
        }

        fn blob_words(&mut self, contents: &[u8], compress: bool) -> [u32; 4] {
            let stored = if compress {
                zlib(contents)
            } else {
                contents.to_vec()
            };
            let offset = self.put(&stored);
            let flag = if compress { FLAG_COMPRESSED } else { 0 };
            [stored.len() as u32 | flag, contents.len() as u32, 0, offset]
        }

        /// A directory listing; directory names are given with their `/`.
        fn dir(&mut self, path: &str, names: &[&str]) {
            let words = self.blob_words(&listing(names), true);
            self.entry(path, 5, vec![(RECORD_DIRECTORY, words.to_vec())]);
        }

        fn build(&self) -> Vec<u8> {
            let mut meta: Vec<u32> = Vec::new();
            let mut table: Vec<(u64, u32, u16, u16)> = Vec::new();
            for (hash, flags, records) in &self.entries {
                let first = meta.len();
                meta.extend(std::iter::repeat_n(0, records.len()));
                for (i, (kind, payload)) in records.iter().enumerate() {
                    meta[first + i] = u32::from(*kind) << 24 | meta.len() as u32;
                    meta.extend(payload);
                }
                table.push((*hash, first as u32, records.len() as u16, *flags));
            }
            table.sort_by_key(|e| e.0);
            let entry_raw: Vec<u8> = table
                .iter()
                .flat_map(|&(h, i, c, f)| {
                    let mut r = h.to_le_bytes().to_vec();
                    r.extend(i.to_le_bytes());
                    r.extend(c.to_le_bytes());
                    r.extend(f.to_le_bytes());
                    r
                })
                .collect();
            let meta_raw: Vec<u8> = meta.iter().flat_map(|w| w.to_le_bytes()).collect();

            let mut file = vec![0u8; DATA_START];
            file.extend(&self.data);
            let entry_z = zlib(&entry_raw);
            let meta_z = zlib(&meta_raw);
            pad16(&mut file);
            let entry_offset = file.len() as u64;
            file.extend(&entry_z);
            pad16(&mut file);
            let meta_offset = file.len() as u64;
            file.extend(&meta_z);

            let mut header = Vec::new();
            header.extend(MAGIC);
            header.extend(2u16.to_le_bytes());
            header.extend(0u16.to_le_bytes());
            header.extend(HASH_METHOD);
            header.extend((table.len() as u32).to_le_bytes());
            header.extend((entry_z.len() as u32).to_le_bytes());
            header.extend((meta.len() as u32).to_le_bytes());
            header.extend((meta_z.len() as u32).to_le_bytes());
            header.extend(entry_offset.to_le_bytes());
            header.extend(meta_offset.to_le_bytes());
            header.extend(0x80u64.to_le_bytes());
            file[..header.len()].copy_from_slice(&header);
            file
        }
    }

    fn pad16(file: &mut Vec<u8>) {
        while !file.len().is_multiple_of(16) {
            file.push(0);
        }
    }

    fn zlib(bytes: &[u8]) -> Vec<u8> {
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    fn listing(names: &[&str]) -> Vec<u8> {
        let mut bytes = (names.len() as u32).to_le_bytes().to_vec();
        bytes.extend(names.iter().map(|n| n.len() as u8));
        for name in names {
            bytes.extend(name.as_bytes());
        }
        bytes
    }

    /// Writes `bytes` to a unique temporary file, removed on drop.
    struct TempArchive(PathBuf);

    impl TempArchive {
        fn new(name: &str, bytes: &[u8]) -> Self {
            let path = std::env::temp_dir().join(format!(
                "specsheet-hashfs-{}-{name}.scs",
                std::process::id()
            ));
            std::fs::write(&path, bytes).unwrap();
            Self(path)
        }

        fn open(&self) -> Result<Archive> {
            Archive::open(&self.0)
        }
    }

    impl Drop for TempArchive {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn open_bytes(name: &str, bytes: &[u8]) -> Result<Archive> {
        TempArchive::new(name, bytes).open()
    }

    fn tiny() -> Builder {
        let mut b = Builder::default();
        b.dir("", &["version.sii", "/def"]);
        b.dir("def", &["a.sii", "raw.txt"]);
        b.file("version.sii", b"SiiNunit\n{\n}\n", true);
        b.file("def/a.sii", &b"x".repeat(5000), true);
        b.file("def/raw.txt", b"stored raw", false);
        b
    }

    fn is_corrupt<T: std::fmt::Debug>(result: Result<T>) -> bool {
        matches!(result, Err(ArchiveError::Corrupt { .. }))
    }

    #[test]
    fn hashes_paths_like_the_game() {
        // version.scs stores these two keys (D:6).
        assert_eq!(hash_path("version.sii"), 0x02d3_3850_6339_918d);
        assert_eq!(hash_path("/version.sii"), 0x02d3_3850_6339_918d);
        assert_eq!(hash_path(""), 0x9ae1_6a3b_2f90_404f);
        assert_eq!(hash_path("/"), hash_path(""));
    }

    #[test]
    fn reads_a_tiny_archive() {
        let temp = TempArchive::new("tiny", &tiny().build());
        let archive = temp.open().unwrap();
        assert_eq!(archive.path(), temp.0.as_path());
        assert_eq!(archive.len(), 5);
        assert!(!archive.is_empty());
        assert_eq!(archive.entries().count(), 5);
        assert!(archive.skipped_entries().is_empty());

        assert_eq!(archive.read("version.sii").unwrap(), b"SiiNunit\n{\n}\n");
        assert_eq!(archive.read("/def/a.sii").unwrap(), b"x".repeat(5000));
        assert_eq!(archive.read("def/raw.txt").unwrap(), b"stored raw");
        let raw = archive.entry("def/raw.txt").unwrap();
        assert_eq!(raw.kind, EntryKind::File);
        assert!(matches!(
            raw.content,
            Content::Stored(Blob {
                compressed: false,
                size: 10,
                ..
            })
        ));

        let root = archive.list_dir("").unwrap();
        assert_eq!(root.files, ["version.sii"]);
        assert_eq!(root.directories, ["def"]);
        assert_eq!(archive.list_dir("/").unwrap(), root);
        assert_eq!(
            archive.list_dir("/def").unwrap().files,
            ["a.sii", "raw.txt"]
        );
        assert_eq!(archive.entry("def").unwrap().kind, EntryKind::Directory);
    }

    #[test]
    fn reports_missing_paths_and_wrong_kinds() {
        let archive = open_bytes("kinds", &tiny().build()).unwrap();
        assert!(archive.entry("nope").is_none());
        assert!(matches!(archive.read("nope"), Err(ArchiveError::NotFound(p)) if p == "nope"));
        assert!(matches!(
            archive.list_dir("nope"),
            Err(ArchiveError::NotFound(_))
        ));
        assert!(matches!(
            archive.read_texture("nope"),
            Err(ArchiveError::NotFound(_))
        ));
        assert!(matches!(
            archive.list_dir("version.sii"),
            Err(ArchiveError::WrongEntryKind {
                expected: "directory",
                ..
            })
        ));
        assert!(matches!(
            archive.read("def"),
            Err(ArchiveError::WrongEntryKind {
                expected: "plain file",
                ..
            })
        ));
        assert!(matches!(
            archive.read_texture("version.sii"),
            Err(ArchiveError::WrongEntryKind {
                expected: "packed texture",
                ..
            })
        ));
    }

    #[test]
    fn concurrent_readers_share_one_archive() {
        let archive = open_bytes("threads", &tiny().build()).unwrap();
        std::thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| {
                    for _ in 0..50 {
                        assert_eq!(archive.read("def/a.sii").unwrap().len(), 5000);
                        assert_eq!(archive.read("def/raw.txt").unwrap(), b"stored raw");
                    }
                });
            }
        });
    }

    #[test]
    fn rejects_foreign_and_unsupported_headers() {
        assert!(matches!(
            open_bytes("empty", b""),
            Err(ArchiveError::NotHashFs)
        ));
        assert!(matches!(
            open_bytes("zip", b"PK\x03\x04rest"),
            Err(ArchiveError::NotHashFs)
        ));
        let mut bad = tiny().build();
        bad[0] = b'X';
        assert!(matches!(
            open_bytes("magic", &bad),
            Err(ArchiveError::NotHashFs)
        ));
        assert!(is_corrupt(open_bytes("short-header", b"SCS#\x02\x00")));

        let good = tiny().build();
        let mut v1 = good.clone();
        v1[4] = 1;
        assert!(matches!(
            open_bytes("v1", &v1),
            Err(ArchiveError::UnsupportedVersion(1))
        ));
        let mut salted = good.clone();
        salted[6] = 7;
        assert!(matches!(
            open_bytes("salt", &salted),
            Err(ArchiveError::UnsupportedSalt(7))
        ));
        let mut fnv = good.clone();
        fnv[8..12].copy_from_slice(b"FNV1");
        assert!(matches!(
            open_bytes("method", &fnv),
            Err(ArchiveError::UnsupportedHashMethod(m)) if m == "FNV1"
        ));
        // An entry table that isn't a deflate zlib stream.
        let mut store = good.clone();
        let entry_offset = le_u64(&store, 0x1c).unwrap() as usize;
        store[entry_offset] = 0x7f;
        assert!(matches!(
            open_bytes("compression", &store),
            Err(ArchiveError::UnsupportedCompression(0x0f))
        ));
    }

    #[test]
    fn rejects_truncated_and_inconsistent_tables() {
        let good = tiny().build();
        let meta_offset = le_u64(&good, 0x24).unwrap() as usize;
        // Cut into the metadata table.
        assert!(is_corrupt(open_bytes("cut", &good[..meta_offset + 4])));

        // A table offset past the end of the file, and an overflowing one.
        for offset in [good.len() as u64 + 16, u64::MAX - 2] {
            let mut far = good.clone();
            far[0x1c..0x24].copy_from_slice(&offset.to_le_bytes());
            assert!(is_corrupt(open_bytes("far", &far)));
        }

        // More entries than the table holds.
        let mut count = good.clone();
        count[0x0c] += 1;
        assert!(is_corrupt(open_bytes("count", &count)));
        // A metadata word count no stored table could inflate to.
        let mut words = good.clone();
        words[0x14..0x18].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(is_corrupt(open_bytes("words", &words)));

        // A record whose payload would run past the metadata table.
        let mut b = tiny();
        b.entry("bad", 0, vec![(RECORD_PLAIN, vec![])]);
        assert!(is_corrupt(open_bytes("short-payload", &b.build())));
    }

    #[test]
    fn rejects_out_of_range_and_mis_sized_blobs() {
        let mut b = tiny();
        b.entry(
            "past-end",
            0,
            vec![(RECORD_PLAIN, vec![100, 100, 0, 0x0fff_ffff])],
        );
        b.entry(
            "overflow",
            0,
            vec![(RECORD_PLAIN, vec![0x0fff_ffff, 1, 0, u32::MAX])],
        );
        let words = b.blob_words(b"hello world", true);
        // Recorded size one byte short, then one byte long.
        b.entry(
            "short",
            0,
            vec![(RECORD_PLAIN, vec![words[0], 10, 0, words[3]])],
        );
        b.entry(
            "long",
            0,
            vec![(RECORD_PLAIN, vec![words[0], 12, 0, words[3]])],
        );
        // A zlib bomb claim: 256 MiB from a few bytes is rejected up front.
        b.entry(
            "bomb",
            0,
            vec![(RECORD_PLAIN, vec![words[0], SIZE_MASK, 0, words[3]])],
        );
        // Raw data whose sizes disagree.
        b.entry("raw", 0, vec![(RECORD_PLAIN, vec![4, 5, 0, words[3]])]);
        // Bytes that don't inflate.
        let junk = b.put(b"not zlib at all");
        b.entry(
            "junk",
            0,
            vec![(RECORD_PLAIN, vec![15 | FLAG_COMPRESSED, 15, 0, junk])],
        );
        let archive = open_bytes("blobs", &b.build()).unwrap();
        for path in [
            "past-end", "overflow", "short", "long", "bomb", "raw", "junk",
        ] {
            assert!(is_corrupt(archive.read(path)), "{path}");
        }
        assert_eq!(archive.read("version.sii").unwrap(), b"SiiNunit\n{\n}\n");
    }

    #[test]
    fn skips_entries_it_cannot_interpret() {
        let mut b = tiny();
        let words = b.blob_words(b"data", false);
        b.entry(
            "unknown-type",
            0,
            vec![(RECORD_PLAIN, words.to_vec()), (0x42, vec![1, 2])],
        );
        b.entry("no-records", 0, vec![]);
        b.entry(
            "two-blobs",
            0,
            vec![
                (RECORD_PLAIN, words.to_vec()),
                (RECORD_PLAIN, words.to_vec()),
            ],
        );
        b.entry(
            "gdeflate-plain",
            0,
            vec![(RECORD_PLAIN, vec![words[0] | 3 << 28, 4, 0, words[3]])],
        );
        b.entry("image-only", 0, vec![(RECORD_IMAGE, vec![0, 0])]);
        // Extra records 0x05/0x06/0x07 don't change how the file is read.
        for (kind, len) in [
            (RECORD_UNKNOWN_05, 8),
            (RECORD_UNKNOWN_06, 2),
            (RECORD_UNKNOWN_07, 2),
        ] {
            let path = format!("extra-{kind}");
            b.entry(
                &path,
                0,
                vec![(RECORD_PLAIN, words.to_vec()), (kind, vec![7; len])],
            );
        }
        let archive = open_bytes("skips", &b.build()).unwrap();
        let reasons: Vec<(u64, &str)> = archive
            .skipped_entries()
            .iter()
            .map(|s| (s.hash, s.reason.as_str()))
            .collect();
        assert_eq!(reasons.len(), 5, "{reasons:?}");
        assert!(reasons.contains(&(
            hash_path("unknown-type"),
            "unknown metadata record type 0x42"
        )));
        for path in ["no-records", "two-blobs", "gdeflate-plain", "image-only"] {
            assert!(archive.entry(path).is_none());
            assert!(reasons.iter().any(|(h, _)| *h == hash_path(path)), "{path}");
        }
        for kind in [5, 6, 7] {
            assert_eq!(archive.read(&format!("extra-{kind}")).unwrap(), b"data");
        }
        assert_eq!(archive.len(), 5 + 3);
    }

    #[test]
    fn rejects_duplicate_hashes() {
        let mut b = tiny();
        b.file("version.sii", b"again", false);
        assert!(is_corrupt(open_bytes("dupes", &b.build())));
    }

    #[test]
    fn decodes_listings() {
        let decoded = decode_listing(&listing(&["b.sii", "/sub", "a.pmd", "/x y"])).unwrap();
        assert_eq!(decoded.files, ["b.sii", "a.pmd"]);
        assert_eq!(decoded.directories, ["sub", "x y"]);
        assert_eq!(
            decode_listing(&listing(&[])).unwrap(),
            DirListing::default()
        );

        let good = listing(&["abc", "/d"]);
        assert!(decode_listing(&good[..good.len() - 1]).is_err());
        assert!(decode_listing(&[good.as_slice(), b"!"].concat()).is_err());
        assert!(decode_listing(&[1, 0, 0]).is_err());
        assert!(decode_listing(&[0xff, 0xff, 0xff, 0xff, 1]).is_err());
        assert!(decode_listing(&[1, 0, 0, 0, 2, 0xc3, 0x28]).is_err());

        // A listing that doesn't decode is `Corrupt` through the archive.
        let mut b = tiny();
        let words = b.blob_words(&good[..good.len() - 1], false);
        b.entry("broken", 5, vec![(RECORD_DIRECTORY, words.to_vec())]);
        let archive = open_bytes("listing", &b.build()).unwrap();
        assert!(is_corrupt(archive.list_dir("broken")));
    }

    /// Image record of a 4x4 BC1 texture, one mip, no padding; and a
    /// linear-filtered, repeating sample record.
    const IMAGE: [u32; 2] = [0x0003_0003, 71 << 4];
    const SAMPLE: u32 = 0b11;

    fn texture_records(blob: [u32; 4]) -> Vec<(u8, Vec<u32>)> {
        vec![
            (RECORD_IMAGE, IMAGE.to_vec()),
            (RECORD_SAMPLE, vec![SAMPLE]),
            (RECORD_MIP_TAIL, blob.to_vec()),
        ]
    }

    #[test]
    fn reads_raw_and_gdeflate_textures() {
        let surface: Vec<u8> = (0..8).collect();
        let mut b = tiny();
        let offset = b.put(&surface);
        // The size word carries 0x3 in its top bits on every texture (D:4).
        let raw = [8, 8 | 3 << 28, 0, offset];
        b.entry("raw.tobj", 0, texture_records(raw));

        let big: Vec<u8> = (0..70_000u32).map(|i| (i % 251) as u8).collect();
        let stream = gdeflate_stream(&big);
        let offset = b.put(&stream);
        let packed = [
            stream.len() as u32 | FLAG_COMPRESSED | FLAG_GDEFLATE,
            big.len() as u32 | 3 << 28,
            0,
            offset,
        ];
        b.entry("packed.tobj", 0, texture_records(packed));
        // A recorded size disagreeing with the stream header.
        let lying = [packed[0], 69_999 | 3 << 28, 0, offset];
        b.entry("lying.tobj", 0, texture_records(lying));
        // A raw texture whose sizes disagree.
        let uneven = [8, 9 | 3 << 28, 0, raw[3]];
        b.entry("uneven.tobj", 0, texture_records(uneven));

        let archive = open_bytes("textures", &b.build()).unwrap();
        assert!(archive.skipped_entries().is_empty());
        let texture = archive.read_texture("raw.tobj").unwrap();
        assert_eq!((texture.image.width, texture.image.height), (4, 4));
        assert_eq!(texture.image.dxgi_format, 71);
        assert!(texture.sample.mag_linear && texture.sample.min_linear);
        assert_eq!(texture.surfaces, surface);
        assert_eq!(texture.unpack().unwrap()[0].data, surface);

        let packed = archive.read_texture("/packed.tobj").unwrap();
        assert_eq!(packed.surfaces, big);
        let Content::Texture { blob, .. } = &archive.entry("packed.tobj").unwrap().content else {
            panic!("not a texture");
        };
        assert!(blob.compressed);
        assert_eq!(blob.size, 70_000);
        assert!(is_corrupt(archive.read_texture("lying.tobj")));
        assert!(is_corrupt(archive.read_texture("uneven.tobj")));
        assert!(matches!(
            archive.read("raw.tobj"),
            Err(ArchiveError::WrongEntryKind {
                expected: "plain file",
                ..
            })
        ));
    }

    /// A DirectStorage GDeflate tile stream (layout in `crate::gdeflate`).
    fn gdeflate_stream(input: &[u8]) -> Vec<u8> {
        let mut compressor = gdeflate::Compressor::new(gdeflate::CompressionLevel::Level6).unwrap();
        let compressed = compressor.compress(input, 64 * 1024).unwrap();
        let tiles = &compressed.tiles;
        let last = tiles.last().unwrap().uncompressed_size;
        let mut stream = vec![4, !4u8];
        stream.extend((tiles.len() as u16).to_le_bytes());
        stream.extend(((last % (64 * 1024)) << 2).to_le_bytes());
        stream.extend(tiles.last().unwrap().compressed_size.to_le_bytes());
        let mut running = 0;
        for tile in &tiles[..tiles.len() - 1] {
            running += tile.compressed_size;
            stream.extend(running.to_le_bytes());
        }
        stream.extend(&compressed.bytes);
        stream
    }
}
