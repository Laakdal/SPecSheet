//! Candidate paths from a folder of mod packages, for [`crate::mod_names`]:
//! the files a ZIP package holds and the paths that any package's files
//! mention. Reading every package takes minutes on a large mod folder (the
//! test install's ProMods alone is 12 packages, 18.5 GB), so each package's
//! result can be cached in a folder the caller names. A cache file is keyed
//! by the package's file name, size and modification time, so a changed
//! package is scanned again. It holds paths only, never file contents. The
//! packages are only read.

use std::fs;
use std::io::Write;
use std::path::Path;

use specsheet_archive::{city_hash_64, HashFsV1Archive, ZipArchive};
use specsheet_formats::paths::referenced_paths;

/// First line of a cache file; a file without it is scanned again.
const CACHE_HEADER: &str = "specsheet mod-scan 1";
/// Entries larger than this are not scanned (as in `mod_names`).
const MAX_SCAN: u64 = 1 << 20;
/// ZIP files whose contents are worth scanning for paths.
const SCANNED: &[&str] = &[".sii", ".sui", ".mat", ".tobj", ".pmd", ".soundref"];

/// The candidates of a folder, and how they were obtained.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FolderScan {
    /// Candidate game paths found in the folder's packages.
    pub paths: Vec<String>,
    /// Packages read in this call.
    pub scanned: usize,
    /// Packages taken from the cache.
    pub cached: usize,
}

/// The paths a package holds (ZIP) or mentions (ZIP and HashFS v1), or
/// `None` when the file is not such a package.
pub fn package_candidates(path: &Path) -> Option<Vec<String>> {
    let mut out = Vec::new();
    if let Ok(zip) = ZipArchive::open(path) {
        for file in zip.files() {
            out.push(file.to_owned());
            let small = zip.size(file).is_some_and(|size| size <= MAX_SCAN);
            if small && SCANNED.iter().any(|ext| file.ends_with(ext)) {
                if let Ok(bytes) = zip.read(file) {
                    out.extend(referenced_paths(file, &bytes));
                }
            }
        }
    } else if let Ok(v1) = HashFsV1Archive::open(path) {
        for entry in v1.entries() {
            if !entry.is_directory() && u64::from(entry.size) <= MAX_SCAN {
                if let Ok(bytes) = v1.read_entry(entry) {
                    out.extend(referenced_paths("", &bytes));
                }
            }
        }
    } else {
        return None;
    }
    Some(out)
}

/// The candidates of every package in `dir`, read from and written to
/// `cache` when given. A cache that cannot be written is skipped.
pub fn folder_candidates(dir: &Path, cache: Option<&Path>) -> std::io::Result<FolderScan> {
    let mut scan = FolderScan::default();
    let mut files: Vec<_> = fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
    files.sort_by_key(|e| e.file_name());
    for entry in files {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let key = cache.map(|c| c.join(cache_name(&entry.file_name().to_string_lossy(), &meta)));
        if let Some(paths) = key.as_deref().and_then(read_cache) {
            scan.paths.extend(paths);
            scan.cached += 1;
            continue;
        }
        let Some(paths) = package_candidates(&path) else {
            continue;
        };
        scan.scanned += 1;
        if let Some(key) = &key {
            let _ = write_cache(key, &paths);
        }
        scan.paths.extend(paths);
    }
    Ok(scan)
}

/// Cache file name for a package: a hash of its name, size and time.
fn cache_name(name: &str, meta: &fs::Metadata) -> String {
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    let key = format!("{name}\0{}\0{modified}", meta.len());
    format!("{:016x}.txt", city_hash_64(key.as_bytes()))
}

fn read_cache(path: &Path) -> Option<Vec<String>> {
    let text = fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    (lines.next()? == CACHE_HEADER).then(|| lines.map(str::to_owned).collect())
}

/// Writes through a temporary file, so a reader never sees half a cache.
fn write_cache(path: &Path, paths: &[String]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("tmp");
    let mut file = fs::File::create(&temp)?;
    writeln!(file, "{CACHE_HEADER}")?;
    for p in paths.iter().filter(|p| !p.contains(['\n', '\r'])) {
        writeln!(file, "{p}")?;
    }
    drop(file);
    fs::rename(&temp, path)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use specsheet_archive::ZipWriter;

    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let unique = format!(
                "specsheet-modscan-{tag}-{}-{:?}",
                std::process::id(),
                std::time::SystemTime::now()
            );
            let path = std::env::temp_dir().join(city_hash_64(unique.as_bytes()).to_string());
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = ZipWriter::new();
        for (path, bytes) in files {
            writer.add(path, bytes).unwrap();
        }
        writer.finish().unwrap()
    }

    #[test]
    fn scans_once_then_reads_the_cache_until_a_package_changes() {
        let dir = TempDir::new("mods");
        let mods = dir.0.join("mod");
        let cache = dir.0.join("cache");
        fs::create_dir_all(&mods).unwrap();
        let package = mods.join("a.scs");
        fs::write(
            &package,
            zip(&[("def/a.sii", b"model: \"/vehicle/x.pmd\"\n")]),
        )
        .unwrap();
        fs::write(mods.join("notes.txt"), b"not a package").unwrap();

        let first = folder_candidates(&mods, Some(&cache)).unwrap();
        assert_eq!((first.scanned, first.cached), (1, 0));
        assert_eq!(first.paths, ["def/a.sii", "vehicle/x.pmd"]);

        let second = folder_candidates(&mods, Some(&cache)).unwrap();
        assert_eq!((second.scanned, second.cached), (0, 1));
        assert_eq!(second.paths, first.paths);

        // A changed package (other size) is read again.
        fs::write(
            &package,
            zip(&[("def/b.sii", b"model: \"/vehicle/longer_name.pmd\"\n")]),
        )
        .unwrap();
        let third = folder_candidates(&mods, Some(&cache)).unwrap();
        assert_eq!((third.scanned, third.cached), (1, 0));
        assert_eq!(third.paths, ["def/b.sii", "vehicle/longer_name.pmd"]);

        // Without a cache every call reads.
        let plain = folder_candidates(&mods, None).unwrap();
        assert_eq!((plain.scanned, plain.cached), (1, 0));
    }

    #[test]
    fn ignores_a_cache_file_without_its_header() {
        let dir = TempDir::new("header");
        let path = dir.0.join("x.txt");
        fs::write(&path, "something else\ndef/a.sii\n").unwrap();
        assert_eq!(read_cache(&path), None);
        write_cache(&path, &["def/a.sii".to_owned(), "bad\nline".to_owned()]).unwrap();
        assert_eq!(read_cache(&path), Some(vec!["def/a.sii".to_owned()]));
    }
}
