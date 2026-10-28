//! HashFS version 1 container (`SCS#` magic, version 1), used by many
//! community mod packages. Layout evidence: `docs/formats/mods.md`
//! ("HashFS v1"), derived from our own byte observations of 20 packages.
//!
//! ```text
//! header (0x14 bytes): "SCS#" | u16 version = 1 | u16 salt | "CITY"
//!                      | u32 entry count | u32 entry table offset
//! entry (32 bytes):    u64 hash | u64 offset | u32 flags | u32 crc
//!                      | u32 size | u32 stored size
//! ```
//!
//! Path keys are CityHash64 of the path without a leading slash; a non-zero
//! salt is prepended as decimal text. Flag bit 0 marks a directory, bit 1 a
//! zlib-compressed entry. Directory listings are text, one name per line,
//! sub-directories prefixed `*`. Packages often omit the root listing, so
//! their files are reachable only by known path.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use flate2::read::ZlibDecoder;

use crate::city::city_hash_64;
use crate::error::{ArchiveError, Result};
use crate::hashfs::DirListing;
use crate::read_at::read_exact_at;

/// Header fields, from `mod-loader-survey.txt` section 2 (20 files).
mod header {
    pub const LEN: usize = 0x14;
    pub const MAGIC: &[u8; 4] = b"SCS#";
    pub const VERSION_AT: usize = 0x04;
    pub const SALT_AT: usize = 0x06;
    pub const METHOD_AT: usize = 0x08;
    pub const METHOD_CITY: &[u8; 4] = b"CITY";
    pub const COUNT_AT: usize = 0x0c;
    pub const TABLE_AT: usize = 0x10;
}

/// Entry record fields, from `hashfs-v1-derivation.txt`.
mod record {
    pub const LEN: usize = 32;
    pub const HASH_AT: usize = 0;
    pub const OFFSET_AT: usize = 8;
    pub const FLAGS_AT: usize = 16;
    // +20 holds a CRC that is not checked (unknown whether the game does).
    pub const SIZE_AT: usize = 24;
    pub const STORED_AT: usize = 28;
}

const FLAG_DIRECTORY: u32 = 1 << 0;
const FLAG_ZLIB: u32 = 1 << 1;
/// Upper bound for one entry we are willing to read or inflate; guards
/// against hostile size fields.
const MAX_ENTRY_SIZE: u32 = 1 << 31;

/// One indexed entry of a v1 archive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntryV1 {
    /// Path hash (salted when the archive has a salt).
    pub hash: u64,
    /// Absolute byte offset of the stored data.
    pub offset: u64,
    /// Entry flags: directory, zlib-compressed.
    pub flags: u32,
    /// Bytes after decompression.
    pub size: u32,
    /// Bytes stored in the archive.
    pub stored_size: u32,
}

impl EntryV1 {
    /// Whether the entry is a directory listing.
    pub fn is_directory(&self) -> bool {
        self.flags & FLAG_DIRECTORY != 0
    }

    /// Whether the stored bytes are zlib-compressed.
    pub fn is_compressed(&self) -> bool {
        self.flags & FLAG_ZLIB != 0
    }
}

/// An opened, indexed, read-only HashFS v1 archive.
#[derive(Debug)]
pub struct HashFsV1Archive {
    path: PathBuf,
    file: File,
    file_len: u64,
    salt: u16,
    by_hash: HashMap<u64, EntryV1>,
}

impl HashFsV1Archive {
    /// Opens and indexes the archive at `path`. Nothing is ever written.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = File::open(&path).map_err(|source| ArchiveError::Io {
            path: path.clone(),
            source,
        })?;
        let file_len = file
            .metadata()
            .map_err(|source| ArchiveError::Io {
                path: path.clone(),
                source,
            })?
            .len();
        let mut archive = Self {
            path,
            file,
            file_len,
            salt: 0,
            by_hash: HashMap::new(),
        };
        archive.load_table()?;
        Ok(archive)
    }

    /// The archive file this was opened from.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The salt prepended to every path before hashing (0: none).
    pub fn salt(&self) -> u16 {
        self.salt
    }

    /// Number of entries (files and directories), the count the game logs.
    pub fn len(&self) -> usize {
        self.by_hash.len()
    }

    /// Whether the archive has no entry.
    pub fn is_empty(&self) -> bool {
        self.by_hash.is_empty()
    }

    /// All entries, in no particular order.
    pub fn entries(&self) -> impl Iterator<Item = &EntryV1> {
        self.by_hash.values()
    }

    /// The key of a game path in this archive.
    pub fn hash_path(&self, path: &str) -> u64 {
        let path = path.trim_start_matches('/');
        if self.salt == 0 {
            city_hash_64(path.as_bytes())
        } else {
            city_hash_64(format!("{}{path}", self.salt).as_bytes())
        }
    }

    /// The entry of a game path, if the archive has it.
    pub fn entry(&self, path: &str) -> Option<&EntryV1> {
        self.by_hash.get(&self.hash_path(path))
    }

    /// Reads and decompresses a file (`def/x.sii` or `/def/x.sii`).
    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        let entry = self.lookup(path)?;
        if entry.is_directory() {
            return Err(ArchiveError::WrongEntryKind {
                path: path.to_owned(),
                expected: "plain file",
            });
        }
        self.read_entry(entry)
    }

    /// Reads the content of any entry (a file, or a directory's listing).
    pub fn read_entry(&self, entry: &EntryV1) -> Result<Vec<u8>> {
        if entry.size > MAX_ENTRY_SIZE || entry.stored_size > MAX_ENTRY_SIZE {
            return Err(self.corrupt(format!("entry {:#018x}: size exceeds limit", entry.hash)));
        }
        let stored = self.bytes_at(entry.offset, u64::from(entry.stored_size))?;
        if !entry.is_compressed() {
            if entry.stored_size != entry.size {
                return Err(self.corrupt(format!(
                    "entry {:#018x}: uncompressed with two sizes",
                    entry.hash
                )));
            }
            return Ok(stored);
        }
        let mut content = Vec::new();
        ZlibDecoder::new(stored.as_slice())
            .take(u64::from(entry.size) + 1)
            .read_to_end(&mut content)
            .map_err(|e| self.corrupt(format!("entry {:#018x}: zlib: {e}", entry.hash)))?;
        if content.len() != entry.size as usize {
            return Err(self.corrupt(format!(
                "entry {:#018x}: inflated to {} bytes, expected {}",
                entry.hash,
                content.len(),
                entry.size
            )));
        }
        Ok(content)
    }

    /// Decodes the listing of a directory (`""` is the root, often absent).
    pub fn list_dir(&self, path: &str) -> Result<DirListing> {
        let entry = self.lookup(path.trim_end_matches('/'))?;
        if !entry.is_directory() {
            return Err(ArchiveError::WrongEntryKind {
                path: path.to_owned(),
                expected: "directory",
            });
        }
        let text = self.read_entry(entry)?;
        let mut listing = DirListing::default();
        for line in String::from_utf8_lossy(&text).lines() {
            let name = line.trim_end_matches('\r');
            match name.strip_prefix('*') {
                Some(sub) if !sub.is_empty() => listing.directories.push(sub.to_owned()),
                Some(_) => {}
                None if !name.is_empty() => listing.files.push(name.to_owned()),
                None => {}
            }
        }
        Ok(listing)
    }

    fn lookup(&self, path: &str) -> Result<&EntryV1> {
        self.entry(path)
            .ok_or_else(|| ArchiveError::NotFound(path.to_owned()))
    }

    fn load_table(&mut self) -> Result<()> {
        let head = self.bytes_at(0, header::LEN as u64).map_err(|_| {
            if self.file_len >= 4 && self.starts_with_magic() {
                self.corrupt("truncated HashFS v1 header".to_owned())
            } else {
                ArchiveError::NotHashFs
            }
        })?;
        if &head[0..4] != header::MAGIC {
            return Err(ArchiveError::NotHashFs);
        }
        let version = le16(&head, header::VERSION_AT);
        if version != 1 {
            return Err(ArchiveError::UnsupportedVersion(version));
        }
        let method = &head[header::METHOD_AT..header::METHOD_AT + 4];
        if method != header::METHOD_CITY {
            return Err(ArchiveError::UnsupportedHashMethod(
                String::from_utf8_lossy(method).into_owned(),
            ));
        }
        self.salt = le16(&head, header::SALT_AT);
        let count = u64::from(le32(&head, header::COUNT_AT));
        let table_at = u64::from(le32(&head, header::TABLE_AT));
        // The count can claim no more records than the file can hold.
        let table_len = count * record::LEN as u64;
        let table = self.bytes_at(table_at, table_len)?;

        self.by_hash.reserve(count as usize);
        for raw in table.chunks_exact(record::LEN) {
            let entry = EntryV1 {
                hash: le64(raw, record::HASH_AT),
                offset: le64(raw, record::OFFSET_AT),
                flags: le32(raw, record::FLAGS_AT),
                size: le32(raw, record::SIZE_AT),
                stored_size: le32(raw, record::STORED_AT),
            };
            self.by_hash.insert(entry.hash, entry);
        }
        Ok(())
    }

    fn starts_with_magic(&self) -> bool {
        let mut magic = [0u8; 4];
        read_exact_at(&self.file, &mut magic, 0).is_ok() && &magic == header::MAGIC
    }

    /// `len` bytes at `offset`, after checking the range lies in the file.
    fn bytes_at(&self, offset: u64, len: u64) -> Result<Vec<u8>> {
        let in_file = offset
            .checked_add(len)
            .is_some_and(|end| end <= self.file_len);
        if !in_file {
            return Err(self.corrupt(format!(
                "{len} bytes at {offset:#x} lie outside the {}-byte file",
                self.file_len
            )));
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

fn le16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn le64(b: &[u8], at: usize) -> u64 {
    let mut word = [0u8; 8];
    word.copy_from_slice(&b[at..at + 8]);
    u64::from_le_bytes(word)
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use flate2::write::ZlibEncoder;
    use flate2::Compression;

    use super::*;
    use crate::city::city_hash_64;

    enum Content {
        File(&'static [u8]),
        Dir(&'static str),
    }

    struct Spec {
        path: &'static str,
        content: Content,
        compress: bool,
    }

    fn file(path: &'static str, data: &'static [u8], compress: bool) -> Spec {
        Spec {
            path,
            content: Content::File(data),
            compress,
        }
    }

    fn dir(path: &'static str, listing: &'static str, compress: bool) -> Spec {
        Spec {
            path,
            content: Content::Dir(listing),
            compress,
        }
    }

    fn key(path: &str, salt: u16) -> u64 {
        if salt == 0 {
            city_hash_64(path.as_bytes())
        } else {
            city_hash_64(format!("{salt}{path}").as_bytes())
        }
    }

    /// Builds a v1 archive: header, data blobs, then the sorted entry table.
    fn build(specs: &[Spec], salt: u16) -> Vec<u8> {
        let mut out = b"SCS#".to_vec();
        out.extend(1u16.to_le_bytes());
        out.extend(salt.to_le_bytes());
        out.extend(b"CITY");
        out.extend((specs.len() as u32).to_le_bytes());
        out.extend(0u32.to_le_bytes()); // table offset, patched below
        let mut records = Vec::new();
        for s in specs {
            let (raw, is_dir) = match s.content {
                Content::File(data) => (data.to_vec(), false),
                Content::Dir(listing) => (listing.as_bytes().to_vec(), true),
            };
            let stored = if s.compress {
                let mut z = ZlibEncoder::new(Vec::new(), Compression::default());
                z.write_all(&raw).unwrap();
                z.finish().unwrap()
            } else {
                raw.clone()
            };
            let flags = u32::from(is_dir) | if s.compress { 2 } else { 0 };
            records.push((
                key(s.path, salt),
                out.len() as u64,
                flags,
                raw.len() as u32,
                stored.len() as u32,
            ));
            out.extend(&stored);
        }
        records.sort_by_key(|r| r.0);
        let table = out.len() as u32;
        out[16..20].copy_from_slice(&table.to_le_bytes());
        for (hash, offset, flags, size, stored) in records {
            out.extend(hash.to_le_bytes());
            out.extend(offset.to_le_bytes());
            out.extend(flags.to_le_bytes());
            out.extend(0u32.to_le_bytes()); // crc, not checked
            out.extend(size.to_le_bytes());
            out.extend(stored.to_le_bytes());
        }
        out
    }

    fn open(bytes: &[u8]) -> Result<HashFsV1Archive> {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("specsheet-v1-{}-{n}.scs", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        let result = HashFsV1Archive::open(&path);
        let _ = std::fs::remove_file(&path);
        result
    }

    fn sample(salt: u16) -> Vec<u8> {
        build(
            &[
                dir("", "*def\nmanifest.sii\n", false),
                dir("def", "*vehicle\r\nsign.sii\r\n\r\n", true),
                file("manifest.sii", b"SiiNunit\n{\n}\n", false),
                file("def/sign.sii", &[b'a'; 300], true),
            ],
            salt,
        )
    }

    #[test]
    fn reads_stored_and_compressed_files_and_text_listings() {
        let archive = open(&sample(0)).unwrap();
        assert_eq!(archive.len(), 4);
        assert_eq!(archive.read("manifest.sii").unwrap(), b"SiiNunit\n{\n}\n");
        assert_eq!(archive.read("/def/sign.sii").unwrap(), vec![b'a'; 300]);
        let root = archive.list_dir("").unwrap();
        assert_eq!(root.directories, ["def"]);
        assert_eq!(root.files, ["manifest.sii"]);
        // CRLF line ends and a trailing empty line, as one producer writes.
        let def = archive.list_dir("/def").unwrap();
        assert_eq!(def.directories, ["vehicle"]);
        assert_eq!(def.files, ["sign.sii"]);
        assert!(archive.entry("def").unwrap().is_directory());
        assert!(archive.entry("def/sign.sii").unwrap().is_compressed());
    }

    #[test]
    fn salted_keys_prepend_the_salt_in_decimal() {
        let archive = open(&sample(0x4914)).unwrap();
        assert_eq!(archive.salt(), 0x4914);
        assert_eq!(
            archive.hash_path("/manifest.sii"),
            city_hash_64(b"18708manifest.sii")
        );
        assert_eq!(archive.read("manifest.sii").unwrap(), b"SiiNunit\n{\n}\n");
    }

    #[test]
    fn files_are_reachable_without_a_root_listing() {
        let bytes = build(&[file("manifest.sii", b"m", false)], 0);
        let archive = open(&bytes).unwrap();
        assert!(matches!(
            archive.list_dir(""),
            Err(ArchiveError::NotFound(_))
        ));
        assert_eq!(archive.read("manifest.sii").unwrap(), b"m");
    }

    #[test]
    fn wrong_entry_kinds_are_reported() {
        let archive = open(&sample(0)).unwrap();
        assert!(matches!(
            archive.read("def"),
            Err(ArchiveError::WrongEntryKind { .. })
        ));
        assert!(matches!(
            archive.list_dir("manifest.sii"),
            Err(ArchiveError::WrongEntryKind { .. })
        ));
        assert!(matches!(
            archive.read("missing.sii"),
            Err(ArchiveError::NotFound(_))
        ));
    }

    #[test]
    fn rejects_other_containers_and_bad_headers() {
        let good = sample(0);
        assert!(matches!(open(b"PK\x03\x04"), Err(ArchiveError::NotHashFs)));
        assert!(matches!(open(b"SCS#"), Err(ArchiveError::Corrupt { .. })));
        let mut v2 = good.clone();
        v2[4] = 2;
        assert!(matches!(
            open(&v2),
            Err(ArchiveError::UnsupportedVersion(2))
        ));
        let mut method = good.clone();
        method[8..12].copy_from_slice(b"FNV1");
        assert!(matches!(
            open(&method),
            Err(ArchiveError::UnsupportedHashMethod(_))
        ));
    }

    #[test]
    fn rejects_an_entry_table_outside_the_file() {
        let mut bytes = sample(0);
        bytes[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(open(&bytes), Err(ArchiveError::Corrupt { .. })));
        let mut count = sample(0);
        count[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(open(&count), Err(ArchiveError::Corrupt { .. })));
    }

    #[test]
    fn rejects_entries_outside_the_file_and_size_mismatches() {
        let bytes = sample(0);
        let table = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
        let record_of = |path: &str| {
            let hash = key(path, 0).to_le_bytes();
            (0..4)
                .map(|i| table + i * 32)
                .find(|&at| bytes[at..at + 8] == hash)
                .unwrap()
        };

        let mut far = bytes.clone();
        let at = record_of("manifest.sii");
        far[at + 8..at + 16].copy_from_slice(&u64::MAX.to_le_bytes());
        let archive = open(&far).unwrap();
        assert!(matches!(
            archive.read("manifest.sii"),
            Err(ArchiveError::Corrupt { .. })
        ));

        let mut wrong_size = bytes.clone();
        let at = record_of("def/sign.sii");
        wrong_size[at + 24..at + 28].copy_from_slice(&301u32.to_le_bytes());
        let archive = open(&wrong_size).unwrap();
        assert!(matches!(
            archive.read("def/sign.sii"),
            Err(ArchiveError::Corrupt { .. })
        ));
    }
}
