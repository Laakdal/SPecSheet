//! A folder on disk read as a package, as the game mounts a Steam Workshop
//! item's unpacked folder (D20, `docs/formats/mods.md`). Read only. Paths
//! are hostile input: a part that is empty, `.`, `..` or holds a drive or
//! stream separator is refused, and a path that resolves outside the folder
//! (through a link) is not found.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{ArchiveError, Result};
use crate::hashfs::DirListing;

/// The largest file read whole, as a sanity bound (a v2 entry holds at
/// most 256 MiB; mods' largest loose files are far smaller).
const MAX_FILE: u64 = 1 << 30;

/// A folder mounted as a package.
#[derive(Debug)]
pub struct FolderPackage {
    root: PathBuf,
}

impl FolderPackage {
    /// Opens the folder `root`. Fails when it is not a folder.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let given = root.as_ref();
        let io = |source| ArchiveError::Io {
            path: given.to_path_buf(),
            source,
        };
        let root = fs::canonicalize(given).map_err(io)?;
        if !root.is_dir() {
            return Err(ArchiveError::WrongEntryKind {
                path: given.display().to_string(),
                expected: "folder",
            });
        }
        Ok(Self { root })
    }

    /// The folder on disk.
    pub fn path(&self) -> &Path {
        &self.root
    }

    /// `path` (leading slash optional, `/`-separated) on disk, when every
    /// part is a plain name and the result stays inside the folder.
    fn resolve(&self, path: &str) -> Option<PathBuf> {
        let path = path.trim_start_matches('/').trim_end_matches('/');
        let mut out = self.root.clone();
        if path.is_empty() {
            return Some(out);
        }
        for part in path.split('/') {
            let plain = !part.is_empty()
                && part != "."
                && part != ".."
                && !part.contains(['\\', ':', '\0']);
            if !plain {
                return None;
            }
            out.push(part);
        }
        // A link may lead outside; only what stays inside is read.
        let real = fs::canonicalize(&out).ok()?;
        real.starts_with(&self.root).then_some(real)
    }

    /// Whether the folder holds the file `path`.
    pub fn is_file(&self, path: &str) -> bool {
        self.resolve(path).is_some_and(|p| p.is_file())
    }

    /// Whether the folder holds the directory `path` (`""` is the root).
    pub fn is_dir(&self, path: &str) -> bool {
        self.resolve(path).is_some_and(|p| p.is_dir())
    }

    /// Reads the file `path`.
    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        let file = self
            .resolve(path)
            .filter(|p| p.is_file())
            .ok_or_else(|| ArchiveError::NotFound(path.to_owned()))?;
        let io = |source| ArchiveError::Io {
            path: file.clone(),
            source,
        };
        let size = fs::metadata(&file).map_err(io)?.len();
        if size > MAX_FILE {
            return Err(ArchiveError::Corrupt {
                path: file.clone(),
                reason: format!("{size} bytes, over the {MAX_FILE}-byte limit"),
            });
        }
        fs::read(&file).map_err(io)
    }

    /// Lists the directory `path`: names sorted, files and folders apart.
    pub fn list_dir(&self, path: &str) -> Result<DirListing> {
        let dir = self
            .resolve(path)
            .filter(|p| p.is_dir())
            .ok_or_else(|| ArchiveError::NotFound(path.to_owned()))?;
        let entries = fs::read_dir(&dir).map_err(|source| ArchiveError::Io {
            path: dir.clone(),
            source,
        })?;
        let mut listing = DirListing::default();
        for entry in entries.flatten() {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => listing.directories.push(name),
                Ok(kind) if kind.is_file() => listing.files.push(name),
                _ => {}
            }
        }
        listing.files.sort();
        listing.directories.sort();
        Ok(listing)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir =
                std::env::temp_dir().join(format!("specsheet-folder-{}-{n}", std::process::id()));
            fs::create_dir_all(dir.join("pkg/def/vehicle")).unwrap();
            fs::write(dir.join("pkg/manifest.sii"), b"m").unwrap();
            fs::write(dir.join("pkg/def/vehicle/a.sii"), b"a").unwrap();
            fs::write(dir.join("outside.sii"), b"secret").unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn reads_and_lists_a_folder_as_a_package() {
        let dir = TempDir::new();
        let pkg = FolderPackage::open(dir.0.join("pkg")).unwrap();
        assert_eq!(pkg.read("/def/vehicle/a.sii").unwrap(), b"a");
        assert!(pkg.is_file("manifest.sii") && !pkg.is_file("def"));
        assert!(pkg.is_dir("") && pkg.is_dir("def/vehicle/") && !pkg.is_dir("manifest.sii"));
        let root = pkg.list_dir("").unwrap();
        assert_eq!(
            (root.files, root.directories),
            (vec!["manifest.sii".to_owned()], vec!["def".to_owned()])
        );
        assert!(matches!(
            pkg.read("def/none.sii"),
            Err(ArchiveError::NotFound(_))
        ));
        assert!(FolderPackage::open(dir.0.join("pkg/manifest.sii")).is_err());
    }

    /// Hostile paths never leave the folder.
    #[test]
    fn refuses_paths_that_leave_the_folder() {
        let dir = TempDir::new();
        let pkg = FolderPackage::open(dir.0.join("pkg")).unwrap();
        let outside = dir.0.join("outside.sii").display().to_string();
        for path in [
            "../outside.sii",
            "def/../../outside.sii",
            "def/./vehicle/a.sii",
            "def//vehicle/a.sii",
            "def\\vehicle\\a.sii",
            "C:/Windows/win.ini",
            outside.as_str(),
        ] {
            assert!(pkg.read(path).is_err(), "{path}");
            assert!(!pkg.is_file(path), "{path}");
        }
        assert!(pkg.list_dir("..").is_err());
    }
}
