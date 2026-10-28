//! ZIP container, used by most community mod packages whatever their
//! extension (`.scs` or `.zip`). Layout evidence: `docs/formats/mods.md`.
//!
//! The reader follows what the game tolerates, not what the ZIP
//! specification demands: some mods set the encryption bit and scramble CRCs
//! and local-header methods to break ordinary ZIP tools, yet the game loads
//! them. Names, method and sizes come from the central directory; only the
//! data offset comes from the local header. CRCs and the encryption bit are
//! ignored.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use flate2::read::DeflateDecoder;

use crate::error::{ArchiveError, Result};
use crate::hashfs::DirListing;
use crate::read_at::read_exact_at;

pub(crate) const END_SIGNATURE: u32 = 0x0605_4b50;
const END_LEN: usize = 22;
const ZIP64_LOCATOR_SIGNATURE: u32 = 0x0706_4b50;
const ZIP64_LOCATOR_LEN: usize = 20;
const ZIP64_END_SIGNATURE: u32 = 0x0606_4b50;
const ZIP64_END_LEN: usize = 56;
const ZIP64_EXTRA_ID: u16 = 0x0001;
pub(crate) const CENTRAL_SIGNATURE: u32 = 0x0201_4b50;
const CENTRAL_LEN: usize = 46;
pub(crate) const LOCAL_SIGNATURE: u32 = 0x0403_4b50;
const LOCAL_LEN: usize = 30;
/// The end record sits within the last `END_LEN + u16::MAX` bytes (its
/// comment is at most 65,535 bytes long).
const END_SEARCH: u64 = (END_LEN + u16::MAX as usize) as u64;
/// Upper bound for one file we are willing to read or inflate; guards
/// against hostile size fields.
const MAX_FILE_SIZE: u64 = 1 << 31;

pub(crate) const METHOD_STORED: u16 = 0;
pub(crate) const METHOD_DEFLATE: u16 = 8;

/// One file of the central directory.
#[derive(Clone, Debug)]
struct ZipEntry {
    method: u16,
    stored_size: u64,
    size: u64,
    local_offset: u64,
}

/// An opened, indexed, read-only ZIP archive.
#[derive(Debug)]
pub struct ZipArchive {
    path: PathBuf,
    file: File,
    file_len: u64,
    records: usize,
    files: HashMap<String, ZipEntry>,
    dirs: BTreeMap<String, DirListing>,
}

impl ZipArchive {
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
            records: 0,
            files: HashMap::new(),
            dirs: BTreeMap::new(),
        };
        archive.index()?;
        Ok(archive)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Number of file entries.
    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Number of central-directory records, directories included. This is
    /// the count the game logs as "Created, N entries".
    pub fn record_count(&self) -> usize {
        self.records
    }

    /// Paths of all files, without a leading slash.
    pub fn files(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }

    pub fn contains(&self, path: &str) -> bool {
        self.files.contains_key(path.trim_start_matches('/'))
    }

    /// Reads and decompresses a file (`def/x.sii` or `/def/x.sii`).
    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        let entry = self
            .files
            .get(path.trim_start_matches('/'))
            .ok_or_else(|| ArchiveError::NotFound(path.to_owned()))?;
        if entry.method != METHOD_STORED && entry.method != METHOD_DEFLATE {
            return Err(ArchiveError::UnsupportedZipMethod {
                path: path.to_owned(),
                method: entry.method,
            });
        }
        if entry.size > MAX_FILE_SIZE || entry.stored_size > MAX_FILE_SIZE {
            return Err(self.corrupt(format!("`{path}`: size exceeds limit")));
        }
        let local = self.read_range(entry.local_offset, LOCAL_LEN as u64)?;
        if u32_le(&local, 0) != LOCAL_SIGNATURE {
            return Err(self.corrupt(format!("`{path}`: bad local header signature")));
        }
        let data_offset = entry.local_offset
            + LOCAL_LEN as u64
            + u64::from(u16_le(&local, 26))
            + u64::from(u16_le(&local, 28));
        let stored = self.read_range(data_offset, entry.stored_size)?;
        if entry.method == METHOD_STORED {
            if entry.stored_size != entry.size {
                return Err(self.corrupt(format!("`{path}`: stored entry with two sizes")));
            }
            return Ok(stored);
        }
        let mut out = Vec::new();
        DeflateDecoder::new(&stored[..])
            .take(entry.size + 1)
            .read_to_end(&mut out)
            .map_err(|e| self.corrupt(format!("`{path}`: deflate: {e}")))?;
        if out.len() as u64 != entry.size {
            return Err(self.corrupt(format!(
                "`{path}`: inflated to {} bytes, expected {}",
                out.len(),
                entry.size
            )));
        }
        Ok(out)
    }

    /// Lists a directory (`""` is the root). Directories need no record of
    /// their own; they are implied by the file paths below them.
    pub fn list_dir(&self, path: &str) -> Result<DirListing> {
        let key = path.trim_matches('/');
        self.dirs
            .get(key)
            .cloned()
            .ok_or_else(|| ArchiveError::NotFound(path.to_owned()))
    }

    fn index(&mut self) -> Result<()> {
        let end = self.find_end()?;
        let (count, cd_size, cd_offset) = self.directory_location(end)?;
        if cd_offset
            .checked_add(cd_size)
            .is_none_or(|stop| stop > self.file_len)
        {
            return Err(self.corrupt(format!(
                "central directory {cd_offset:#x}+{cd_size} exceeds file length"
            )));
        }
        // Every record is at least CENTRAL_LEN bytes, so the declared count
        // cannot exceed what the directory can hold.
        if count > cd_size / CENTRAL_LEN as u64 {
            return Err(self.corrupt(format!(
                "{count} records cannot fit in a {cd_size}-byte central directory"
            )));
        }
        let cd = self.read_range(cd_offset, cd_size)?;

        let mut dirs: BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)> = BTreeMap::new();
        dirs.entry(String::new()).or_default();
        let mut cursor = 0usize;
        for record in 0..count {
            let raw = cd
                .get(cursor..cursor + CENTRAL_LEN)
                .ok_or_else(|| self.corrupt(format!("record #{record} truncated")))?;
            if u32_le(raw, 0) != CENTRAL_SIGNATURE {
                return Err(self.corrupt(format!("record #{record}: bad signature")));
            }
            let name_len = usize::from(u16_le(raw, 28));
            let extra_len = usize::from(u16_le(raw, 30));
            let comment_len = usize::from(u16_le(raw, 32));
            let name_start = cursor + CENTRAL_LEN;
            let extra_start = name_start + name_len;
            let next = extra_start + extra_len + comment_len;
            let name = cd
                .get(name_start..extra_start)
                .ok_or_else(|| self.corrupt(format!("record #{record}: name truncated")))?;
            let extra = cd
                .get(extra_start..extra_start + extra_len)
                .ok_or_else(|| self.corrupt(format!("record #{record}: extra truncated")))?;
            cursor = next;

            let name = String::from_utf8_lossy(name).into_owned();
            let mut entry = ZipEntry {
                method: u16_le(raw, 10),
                stored_size: u64::from(u32_le(raw, 20)),
                size: u64::from(u32_le(raw, 24)),
                local_offset: u64::from(u32_le(raw, 42)),
            };
            apply_zip64_extra(&mut entry, extra)
                .map_err(|reason| self.corrupt(format!("record #{record}: {reason}")))?;
            self.records += 1;
            register(&mut dirs, &name);
            if !name.ends_with('/') {
                self.files.insert(name, entry);
            }
        }
        self.dirs = dirs
            .into_iter()
            .map(|(dir, (directories, files))| {
                let listing = DirListing {
                    files: files.into_iter().collect(),
                    directories: directories.into_iter().collect(),
                };
                (dir, listing)
            })
            .collect();
        Ok(())
    }

    /// Offset of the end-of-central-directory record, searched backwards.
    fn find_end(&self) -> Result<u64> {
        if self.file_len < END_LEN as u64 {
            return Err(ArchiveError::NotZip);
        }
        let window = self.file_len.min(END_SEARCH);
        let start = self.file_len - window;
        let tail = self.read_range(start, window)?;
        (0..=tail.len() - END_LEN)
            .rev()
            .find(|&at| u32_le(&tail, at) == END_SIGNATURE)
            .map(|at| start + at as u64)
            .ok_or(ArchiveError::NotZip)
    }

    /// Record count, size and offset of the central directory, from the end
    /// record or, when its fields are saturated, the ZIP64 end record.
    fn directory_location(&self, end: u64) -> Result<(u64, u64, u64)> {
        let raw = self.read_range(end, END_LEN as u64)?;
        let count = u16_le(&raw, 10);
        let size = u32_le(&raw, 12);
        let offset = u32_le(&raw, 16);
        if count != u16::MAX && size != u32::MAX && offset != u32::MAX {
            return Ok((u64::from(count), u64::from(size), u64::from(offset)));
        }
        let locator_at = end
            .checked_sub(ZIP64_LOCATOR_LEN as u64)
            .ok_or_else(|| self.corrupt("missing ZIP64 locator".to_owned()))?;
        let locator = self.read_range(locator_at, ZIP64_LOCATOR_LEN as u64)?;
        if u32_le(&locator, 0) != ZIP64_LOCATOR_SIGNATURE {
            // Saturated fields without ZIP64: take them at face value.
            return Ok((u64::from(count), u64::from(size), u64::from(offset)));
        }
        let end64 = self.read_range(u64_le(&locator, 8), ZIP64_END_LEN as u64)?;
        if u32_le(&end64, 0) != ZIP64_END_SIGNATURE {
            return Err(self.corrupt("bad ZIP64 end signature".to_owned()));
        }
        Ok((u64_le(&end64, 32), u64_le(&end64, 40), u64_le(&end64, 48)))
    }

    fn read_range(&self, offset: u64, len: u64) -> Result<Vec<u8>> {
        if offset
            .checked_add(len)
            .is_none_or(|end| end > self.file_len)
        {
            return Err(self.corrupt(format!("range {offset:#x}+{len} exceeds file length")));
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

/// Adds `name` (a file, or a directory ending in `/`) and all its parent
/// directories to the listings.
fn register(dirs: &mut BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)>, name: &str) {
    let is_dir = name.ends_with('/');
    let trimmed = name.trim_end_matches('/');
    if trimmed.is_empty() {
        return;
    }
    let mut parts: Vec<&str> = trimmed.split('/').collect();
    let leaf = parts.pop().unwrap_or_default();
    let mut parent = String::new();
    for part in parts {
        let listing = dirs.entry(parent.clone()).or_default();
        listing.0.insert(part.to_owned());
        if !parent.is_empty() {
            parent.push('/');
        }
        parent.push_str(part);
    }
    let listing = dirs.entry(parent.clone()).or_default();
    if is_dir {
        listing.0.insert(leaf.to_owned());
        let path = if parent.is_empty() {
            leaf.to_owned()
        } else {
            format!("{parent}/{leaf}")
        };
        dirs.entry(path).or_default();
    } else {
        listing.1.insert(leaf.to_owned());
    }
}

/// Replaces saturated 32-bit fields with the 64-bit values of the ZIP64
/// extra field, which stores only the saturated ones, in this order.
fn apply_zip64_extra(entry: &mut ZipEntry, mut extra: &[u8]) -> std::result::Result<(), String> {
    while extra.len() >= 4 {
        let id = u16_le(extra, 0);
        let len = usize::from(u16_le(extra, 2));
        let data = extra
            .get(4..4 + len)
            .ok_or_else(|| "extra field truncated".to_owned())?;
        if id == ZIP64_EXTRA_ID {
            let mut at = 0;
            for field in [
                &mut entry.size,
                &mut entry.stored_size,
                &mut entry.local_offset,
            ] {
                if *field == u64::from(u32::MAX) {
                    let value = data
                        .get(at..at + 8)
                        .ok_or_else(|| "ZIP64 extra field truncated".to_owned())?;
                    *field = u64_le(value, 0);
                    at += 8;
                }
            }
        }
        extra = &extra[4 + len..];
    }
    Ok(())
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
    use std::io::Write;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use flate2::write::DeflateEncoder;
    use flate2::Compression;

    use super::*;

    /// One entry of a fixture archive, with knobs for the malformations seen
    /// in real mods.
    struct Spec {
        name: &'static str,
        data: Vec<u8>,
        deflate: bool,
        central_flags: u16,
        central_crc: u32,
        local_method: Option<u16>,
        local_crc: u32,
        central_method: Option<u16>,
    }

    fn spec(name: &'static str, data: &[u8], deflate: bool) -> Spec {
        Spec {
            name,
            data: data.to_vec(),
            deflate,
            central_flags: 0,
            central_crc: 0,
            local_method: None,
            local_crc: 0,
            central_method: None,
        }
    }

    fn deflated(data: &[u8]) -> Vec<u8> {
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

    /// Builds a ZIP: local headers with data, central directory, end record.
    fn build(specs: &[Spec]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for s in specs {
            let stored = if s.deflate {
                deflated(&s.data)
            } else {
                s.data.clone()
            };
            let method: u16 = if s.deflate { 8 } else { 0 };
            let offset = out.len() as u32;
            out.extend(0x0403_4b50u32.to_le_bytes());
            out.extend(20u16.to_le_bytes());
            out.extend(0u16.to_le_bytes());
            out.extend(s.local_method.unwrap_or(method).to_le_bytes());
            out.extend([0u8; 4]); // time, date
            out.extend(s.local_crc.to_le_bytes());
            out.extend((stored.len() as u32).to_le_bytes());
            out.extend((s.data.len() as u32).to_le_bytes());
            out.extend((s.name.len() as u16).to_le_bytes());
            out.extend(0u16.to_le_bytes());
            out.extend(s.name.as_bytes());
            out.extend(&stored);

            central.extend(0x0201_4b50u32.to_le_bytes());
            central.extend(20u16.to_le_bytes());
            central.extend(20u16.to_le_bytes());
            central.extend(s.central_flags.to_le_bytes());
            central.extend(s.central_method.unwrap_or(method).to_le_bytes());
            central.extend([0u8; 4]);
            central.extend(s.central_crc.to_le_bytes());
            central.extend((stored.len() as u32).to_le_bytes());
            central.extend((s.data.len() as u32).to_le_bytes());
            central.extend((s.name.len() as u16).to_le_bytes());
            central.extend(0u16.to_le_bytes()); // extra
            central.extend(0u16.to_le_bytes()); // comment
            central.extend([0u8; 8]); // disk, internal, external attributes
            central.extend(offset.to_le_bytes());
            central.extend(s.name.as_bytes());
        }
        let cd_offset = out.len() as u32;
        out.extend(&central);
        out.extend(0x0605_4b50u32.to_le_bytes());
        out.extend([0u8; 4]);
        out.extend((specs.len() as u16).to_le_bytes());
        out.extend((specs.len() as u16).to_le_bytes());
        out.extend((central.len() as u32).to_le_bytes());
        out.extend(cd_offset.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out
    }

    fn temp(bytes: &[u8]) -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("specsheet-zip-{}-{n}.zip", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn open(bytes: &[u8]) -> Result<ZipArchive> {
        let path = temp(bytes);
        let result = ZipArchive::open(&path);
        let _ = std::fs::remove_file(&path);
        result
    }

    #[test]
    fn reads_stored_and_deflated_files_and_lists_implied_directories() {
        let text = b"SiiNunit\n{\n}\n".repeat(20);
        let zip = open(&build(&[
            spec("manifest.sii", &text, true),
            spec("def/", b"", false),
            spec("def/vehicle/truck/a.sii", b"plain", false),
            spec("icon.jpg", b"\xff\xd8", false),
        ]))
        .unwrap();
        assert_eq!(zip.record_count(), 4);
        assert_eq!(zip.len(), 3);
        assert_eq!(zip.read("manifest.sii").unwrap(), text);
        assert_eq!(zip.read("/def/vehicle/truck/a.sii").unwrap(), b"plain");
        let root = zip.list_dir("").unwrap();
        assert_eq!(root.directories, ["def"]);
        assert_eq!(root.files, ["icon.jpg", "manifest.sii"]);
        // `def/vehicle` has no record of its own.
        assert_eq!(zip.list_dir("/def/vehicle").unwrap().directories, ["truck"]);
        assert!(matches!(
            zip.read("missing.sii"),
            Err(ArchiveError::NotFound(_))
        ));
    }

    #[test]
    fn ignores_encryption_bit_crcs_and_local_method() {
        // The shape of a protected mod the game loads (docs/formats/mods.md).
        let text = b"mod_package : .p {\n}\n".repeat(10);
        let mut s = spec("manifest.sii", &text, true);
        s.central_flags = 1;
        s.central_crc = 0x676f_6f77;
        s.local_crc = 0xf4fc_fc04;
        s.local_method = Some(1);
        let zip = open(&build(&[s])).unwrap();
        assert_eq!(zip.read("manifest.sii").unwrap(), text);
    }

    #[test]
    fn unsupported_method_fails_only_that_entry() {
        let mut odd = spec("odd.bin", b"xxxx", false);
        odd.central_method = Some(14); // LZMA
        let zip = open(&build(&[odd, spec("ok.txt", b"ok", false)])).unwrap();
        assert!(matches!(
            zip.read("odd.bin"),
            Err(ArchiveError::UnsupportedZipMethod { method: 14, .. })
        ));
        assert_eq!(zip.read("ok.txt").unwrap(), b"ok");
    }

    #[test]
    fn rejects_files_without_an_end_record() {
        assert!(matches!(
            open(b"PK\x03\x04 short"),
            Err(ArchiveError::NotZip)
        ));
        assert!(matches!(open(b""), Err(ArchiveError::NotZip)));
    }

    #[test]
    fn rejects_out_of_range_central_directory_and_counts() {
        let good = build(&[spec("a.txt", b"a", false)]);
        let end = good.len() - 22;

        let mut bad_offset = good.clone();
        bad_offset[end + 16..end + 20].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            open(&bad_offset),
            Err(ArchiveError::Corrupt { .. })
        ));

        let mut too_many = good.clone();
        too_many[end + 8..end + 10].copy_from_slice(&u16::MAX.to_le_bytes());
        too_many[end + 10..end + 12].copy_from_slice(&u16::MAX.to_le_bytes());
        assert!(matches!(open(&too_many), Err(ArchiveError::Corrupt { .. })));
    }

    #[test]
    fn rejects_entries_whose_data_lies_outside_the_file() {
        let mut bytes = build(&[spec("a.txt", b"abc", false)]);
        let end = bytes.len() - 22;
        let cd = u32::from_le_bytes(bytes[end + 16..end + 20].try_into().unwrap()) as usize;
        // Local header offset of record 0 points past the end of the file.
        bytes[cd + 42..cd + 46].copy_from_slice(&u32::MAX.to_le_bytes());
        let zip = open(&bytes).unwrap();
        assert!(matches!(
            zip.read("a.txt"),
            Err(ArchiveError::Corrupt { .. })
        ));
    }

    #[test]
    fn rejects_inflated_size_mismatch() {
        let mut bytes = build(&[spec("a.txt", &b"abc".repeat(50), true)]);
        let end = bytes.len() - 22;
        let cd = u32::from_le_bytes(bytes[end + 16..end + 20].try_into().unwrap()) as usize;
        // Central directory claims one byte more than the stream inflates to.
        bytes[cd + 24..cd + 28].copy_from_slice(&151u32.to_le_bytes());
        let zip = open(&bytes).unwrap();
        assert!(matches!(
            zip.read("a.txt"),
            Err(ArchiveError::Corrupt { .. })
        ));
    }
}
