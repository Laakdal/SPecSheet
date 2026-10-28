//! Writes new ZIP mod packages in memory (D18). Layout evidence:
//! `docs/formats/mods.md`.
//!
//! The output is a plain, standard ZIP that any tool opens: correct CRCs,
//! no encryption bit, sizes in both headers and no data descriptors. It stays
//! within what the game is seen to load: stored or deflated files, no ZIP64,
//! and no directory records (a package without them mounts, F14). The
//! timestamp is fixed, so the same files always give the same bytes.

use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use flate2::write::DeflateEncoder;
use flate2::{Compression, Crc};

use crate::error::{ArchiveError, Result};
use crate::zip::{
    CENTRAL_SIGNATURE, END_SIGNATURE, LOCAL_SIGNATURE, METHOD_DEFLATE, METHOD_STORED,
};

/// ZIP 2.0: deflate, no ZIP64.
const VERSION: u16 = 20;
/// 1980-01-01 00:00, the earliest DOS date.
const DOS_DATE: u16 = (1 << 5) | 1;
const DOS_TIME: u16 = 0;
/// `0xffff` entries or `0xffff_ffff` bytes would need ZIP64 records.
const MAX_ENTRIES: usize = u16::MAX as usize - 1;
const MAX_OFFSET: u64 = u32::MAX as u64 - 1;

/// Builds a ZIP package in memory. Each file is deflated, or stored when
/// deflating does not make it smaller.
#[derive(Debug, Default)]
pub struct ZipWriter {
    out: Vec<u8>,
    central: Vec<u8>,
    names: HashSet<String>,
}

impl ZipWriter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of files added so far.
    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// Adds a file at `path`, a game path without a leading slash
    /// (`def/x.sii`). Rejects unsafe or duplicate paths and anything that
    /// would need ZIP64.
    pub fn add(&mut self, path: &str, data: &[u8]) -> Result<()> {
        check_path(path)?;
        if self.names.contains(path) {
            return Err(ArchiveError::DuplicatePackagePath(path.to_owned()));
        }
        if self.names.len() >= MAX_ENTRIES {
            return Err(too_large(format!("more than {MAX_ENTRIES} files")));
        }
        let mut crc = Crc::new();
        crc.update(data);
        let crc = crc.sum();
        let (method, stored) = compress(data)?;
        let offset = self.out.len() as u64;
        let local_len = 30 + path.len() as u64 + stored.len() as u64;
        if data.len() as u64 > MAX_OFFSET || offset + local_len > MAX_OFFSET {
            return Err(too_large(format!("`{path}` passes 4 GiB")));
        }
        let header = Header {
            method,
            crc,
            stored_size: stored.len() as u32,
            size: data.len() as u32,
            name: path,
        };
        put_u32(&mut self.out, LOCAL_SIGNATURE);
        put_u16(&mut self.out, VERSION);
        header.write(&mut self.out);
        self.out.extend_from_slice(&stored);

        put_u32(&mut self.central, CENTRAL_SIGNATURE);
        put_u16(&mut self.central, VERSION); // made by: MS-DOS, 2.0
        put_u16(&mut self.central, VERSION);
        header.write_fields(&mut self.central);
        put_u16(&mut self.central, 0); // comment length
        put_u16(&mut self.central, 0); // disk number
        put_u16(&mut self.central, 0); // internal attributes
        put_u32(&mut self.central, 0); // external attributes
        put_u32(&mut self.central, offset as u32);
        self.central.extend_from_slice(path.as_bytes());
        self.names.insert(path.to_owned());
        Ok(())
    }

    /// The finished archive.
    pub fn finish(mut self) -> Result<Vec<u8>> {
        let offset = self.out.len() as u64;
        let size = self.central.len() as u64;
        if offset + size + 22 > MAX_OFFSET {
            return Err(too_large("the archive passes 4 GiB".to_owned()));
        }
        let count = self.names.len() as u16;
        self.out.append(&mut self.central);
        put_u32(&mut self.out, END_SIGNATURE);
        put_u16(&mut self.out, 0); // this disk
        put_u16(&mut self.out, 0); // disk with the central directory
        put_u16(&mut self.out, count);
        put_u16(&mut self.out, count);
        put_u32(&mut self.out, size as u32);
        put_u32(&mut self.out, offset as u32);
        put_u16(&mut self.out, 0); // comment length
        Ok(self.out)
    }
}

/// Writes a finished package into `dir` as `file_name` (`my_mod.scs` or
/// `my_mod.zip`) and returns its path. Never replaces anything: the game
/// names a package by its file stem, so this fails when any entry of `dir`
/// already has that stem, ignoring case. The file is created once at its
/// final name (no temporary file the game could pick up as a mod) and
/// removed again if writing fails.
pub fn write_new_package(dir: &Path, file_name: &str, bytes: &[u8]) -> Result<PathBuf> {
    let stem = file_name
        .strip_suffix(".scs")
        .or_else(|| file_name.strip_suffix(".zip"))
        .filter(|stem| {
            !stem.is_empty() && !stem.contains(['/', '\\', ':']) && !stem.starts_with('.')
        })
        .ok_or_else(|| ArchiveError::InvalidPackagePath {
            path: file_name.to_owned(),
            reason: "not a `<name>.scs` or `<name>.zip` file name",
        })?;
    let io_error = |path: &Path| {
        let path = path.to_path_buf();
        move |source| ArchiveError::Io { path, source }
    };
    for entry in std::fs::read_dir(dir).map_err(io_error(dir))? {
        let existing = entry.map_err(io_error(dir))?.path();
        let same = existing
            .file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case(stem));
        if same {
            return Err(ArchiveError::PackageExists(existing));
        }
    }
    let path = dir.join(file_name);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(io_error(&path))?;
    if let Err(source) = file.write_all(bytes).and_then(|()| file.sync_all()) {
        drop(file);
        let _ = std::fs::remove_file(&path);
        return Err(ArchiveError::Io { path, source });
    }
    Ok(path)
}

/// The fields local and central headers share, from "flags" to "extra
/// length", plus the name.
struct Header<'a> {
    method: u16,
    crc: u32,
    stored_size: u32,
    size: u32,
    name: &'a str,
}

impl Header<'_> {
    /// Local header form: the fields, then the name.
    fn write(&self, out: &mut Vec<u8>) {
        self.write_fields(out);
        out.extend_from_slice(self.name.as_bytes());
    }

    fn write_fields(&self, out: &mut Vec<u8>) {
        put_u16(out, 0); // flags: no encryption, no data descriptor
        put_u16(out, self.method);
        put_u16(out, DOS_TIME);
        put_u16(out, DOS_DATE);
        put_u32(out, self.crc);
        put_u32(out, self.stored_size);
        put_u32(out, self.size);
        put_u16(out, self.name.len() as u16);
        put_u16(out, 0); // extra length
    }
}

/// Deflated bytes when they are smaller, else the input unchanged.
fn compress(data: &[u8]) -> Result<(u16, Vec<u8>)> {
    if data.is_empty() {
        return Ok((METHOD_STORED, Vec::new()));
    }
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
    let deflated = encoder
        .write_all(data)
        .and_then(|()| encoder.finish())
        .map_err(|e| too_large(format!("deflate failed: {e}")))?;
    Ok(if deflated.len() < data.len() {
        (METHOD_DEFLATE, deflated)
    } else {
        (METHOD_STORED, data.to_vec())
    })
}

/// A package path is relative, ASCII and `/`-separated, with no empty,
/// `.` or `..` component, so it cannot escape the package when unpacked.
fn check_path(path: &str) -> Result<()> {
    let reason = if path.is_empty() {
        Some("empty")
    } else if path.len() > usize::from(u16::MAX) {
        Some("longer than 65,535 bytes")
    } else if !path.bytes().all(|b| b.is_ascii() && !b.is_ascii_control()) {
        Some("not printable ASCII")
    } else if path.contains('\\') || path.contains(':') {
        Some("contains `\\` or `:`")
    } else if path
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        Some("has a leading, trailing or doubled `/`, or a `.` or `..` part")
    } else {
        None
    };
    match reason {
        Some(reason) => Err(ArchiveError::InvalidPackagePath {
            path: path.to_owned(),
            reason,
        }),
        None => Ok(()),
    }
}

fn too_large(reason: String) -> ArchiveError {
    ArchiveError::PackageTooLarge(reason)
}

fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use flate2::read::DeflateDecoder;

    use super::*;
    use crate::ZipArchive;

    const TEXT: &[u8] = b"SiiNunit\n{\nmod_package : .test\n{\n display_name: \"Test\"\n}\n}\n";

    /// Bytes deflate cannot shrink (a simple LCG).
    fn noise(len: usize) -> Vec<u8> {
        let mut state = 0x2545_f491_u32;
        (0..len)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 24) as u8
            })
            .collect()
    }

    fn package(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = ZipWriter::new();
        for (path, data) in files {
            writer.add(path, data).unwrap();
        }
        writer.finish().unwrap()
    }

    fn temp_file(bytes: &[u8]) -> std::path::PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "specsheet-zipw-{}-{}.zip",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn u16_at(bytes: &[u8], at: usize) -> u16 {
        u16::from_le_bytes([bytes[at], bytes[at + 1]])
    }

    fn u32_at(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
    }

    /// A strict check of the parts our tolerant reader ignores: every CRC
    /// matches the data, the local header repeats the central one, no flag
    /// is set and the end record accounts for every byte.
    fn assert_strict(bytes: &[u8]) -> Vec<(String, u16)> {
        let end = bytes.len() - 22;
        assert_eq!(u32_at(bytes, end), END_SIGNATURE);
        let count = usize::from(u16_at(bytes, end + 10));
        assert_eq!(u16_at(bytes, end + 8), count as u16);
        let cd_size = u32_at(bytes, end + 12) as usize;
        let cd_offset = u32_at(bytes, end + 16) as usize;
        assert_eq!(cd_offset + cd_size, end);
        let mut entries = Vec::new();
        let mut at = cd_offset;
        let mut local_at = 0;
        for _ in 0..count {
            assert_eq!(u32_at(bytes, at), CENTRAL_SIGNATURE);
            let fields = &bytes[at + 8..at + 32];
            assert_eq!(u16_at(bytes, at + 8), 0, "central flags");
            let name_len = usize::from(u16_at(bytes, at + 28));
            assert_eq!(u16_at(bytes, at + 30), 0, "extra length");
            assert_eq!(u16_at(bytes, at + 32), 0, "comment length");
            let offset = u32_at(bytes, at + 42) as usize;
            assert_eq!(offset, local_at, "files are packed in order");
            let name = &bytes[at + 46..at + 46 + name_len];

            assert_eq!(u32_at(bytes, offset), LOCAL_SIGNATURE);
            assert_eq!(&bytes[offset + 6..offset + 30], fields, "local = central");
            assert_eq!(&bytes[offset + 30..offset + 30 + name_len], name);
            let method = u16_at(fields, 2);
            let crc = u32_at(fields, 8);
            let stored_size = u32_at(fields, 12) as usize;
            let size = u32_at(fields, 16) as usize;
            let data_at = offset + 30 + name_len;
            let stored = &bytes[data_at..data_at + stored_size];
            let data = match method {
                METHOD_STORED => stored.to_vec(),
                METHOD_DEFLATE => {
                    let mut out = Vec::new();
                    DeflateDecoder::new(stored).read_to_end(&mut out).unwrap();
                    out
                }
                other => panic!("method {other}"),
            };
            assert_eq!(data.len(), size);
            let mut check = Crc::new();
            check.update(&data);
            assert_eq!(check.sum(), crc, "CRC");
            local_at = data_at + stored_size;
            at += 46 + name_len;
            entries.push((String::from_utf8(name.to_vec()).unwrap(), method));
        }
        assert_eq!(local_at, cd_offset, "nothing between files and directory");
        entries
    }

    #[test]
    fn writes_a_standard_zip_that_our_reader_reads_back() {
        let image = noise(4096);
        let bytes = package(&[
            ("manifest.sii", TEXT),
            ("def/vehicle/x.sii", &TEXT.repeat(20)),
            ("icon.jpg", &image),
            ("empty.txt", b""),
        ]);
        let entries = assert_strict(&bytes);
        // The short manifest and the noise do not shrink, so they are stored.
        assert_eq!(
            entries,
            [
                ("manifest.sii".to_owned(), METHOD_STORED),
                ("def/vehicle/x.sii".to_owned(), METHOD_DEFLATE),
                ("icon.jpg".to_owned(), METHOD_STORED),
                ("empty.txt".to_owned(), METHOD_STORED),
            ]
        );
        let path = temp_file(&bytes);
        let archive = ZipArchive::open(&path).unwrap();
        assert_eq!(archive.record_count(), 4, "no directory records");
        assert_eq!(archive.read("/manifest.sii").unwrap(), TEXT);
        assert_eq!(archive.read("def/vehicle/x.sii").unwrap(), TEXT.repeat(20));
        assert_eq!(archive.read("icon.jpg").unwrap(), image);
        assert_eq!(archive.read("empty.txt").unwrap(), b"");
        assert_eq!(archive.list_dir("def").unwrap().directories, ["vehicle"]);
        drop(archive);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn output_is_deterministic() {
        let files: &[(&str, &[u8])] = &[("a.txt", b"a"), ("b/c.sii", TEXT)];
        assert_eq!(package(files), package(files));
    }

    #[test]
    fn an_empty_package_is_just_an_end_record() {
        let bytes = ZipWriter::new().finish().unwrap();
        assert_eq!(bytes.len(), 22);
        assert!(assert_strict(&bytes).is_empty());
    }

    #[test]
    fn rejects_unsafe_paths() {
        for path in [
            "",
            "/def/x.sii",
            "def/",
            "def//x.sii",
            "./x.sii",
            "def/../x.sii",
            "..",
            "def\\x.sii",
            "C:/x.sii",
            "d\u{e9}f.sii",
            "def/x\n.sii",
        ] {
            assert!(
                matches!(
                    ZipWriter::new().add(path, b"x"),
                    Err(ArchiveError::InvalidPackagePath { .. })
                ),
                "{path:?}"
            );
        }
        assert!(ZipWriter::new().add("def/my mod/x_1.sii", b"x").is_ok());
    }

    #[test]
    fn rejects_duplicates_and_zip64_counts() {
        let mut writer = ZipWriter::new();
        writer.add("a.txt", b"a").unwrap();
        assert!(matches!(
            writer.add("a.txt", b"b"),
            Err(ArchiveError::DuplicatePackagePath(_))
        ));
        assert_eq!(writer.len(), 1);

        let mut writer = ZipWriter::new();
        for i in 0..MAX_ENTRIES {
            writer.add(&format!("f{i}"), b"").unwrap();
        }
        assert!(matches!(
            writer.add("one_more", b""),
            Err(ArchiveError::PackageTooLarge(_))
        ));
        let bytes = writer.finish().unwrap();
        assert_eq!(u16_at(&bytes, bytes.len() - 12), MAX_ENTRIES as u16);
    }

    fn temp_dir() -> std::path::PathBuf {
        let file = temp_file(b"");
        std::fs::remove_file(&file).unwrap();
        let dir = file.with_extension("d");
        std::fs::create_dir(&dir).unwrap();
        dir
    }

    #[test]
    fn writes_a_new_package_and_never_replaces_one() {
        let dir = temp_dir();
        let bytes = package(&[("manifest.sii", TEXT)]);
        let path = write_new_package(&dir, "my_mod.scs", &bytes).unwrap();
        assert_eq!(path, dir.join("my_mod.scs"));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);

        // Same name, or the same package name (stem) in another case or
        // extension: refused, and the first file is untouched.
        for name in ["my_mod.scs", "My_Mod.zip"] {
            assert!(
                matches!(
                    write_new_package(&dir, name, b"other"),
                    Err(ArchiveError::PackageExists(_))
                ),
                "{name}"
            );
        }
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);

        for name in [
            "my_mod.txt",
            ".scs",
            "a/b.scs",
            r"a\b.zip",
            "C:x.scs",
            ".hidden.scs",
        ] {
            assert!(
                matches!(
                    write_new_package(&dir, name, &bytes),
                    Err(ArchiveError::InvalidPackagePath { .. })
                ),
                "{name}"
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
