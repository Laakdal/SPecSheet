//! A layered, read-only view over several mounted packages, mirroring the
//! game's file system: packages mounted later override files of the same
//! path, and directory listings are merged across all packages.
//!
//! A package is a game archive (HashFS v2) or a mod package (HashFS v1,
//! ZIP, or a Steam Workshop item's folder, D20). A mod package may be mounted below a root folder, as the game
//! does for the `base/` and `dlc_<name>/` folders of a multi-folder package:
//! the package's `base/def/x.sii` is then the game path `def/x.sii`.

use std::collections::BTreeSet;
use std::path::Path;

use crate::error::{ArchiveError, Result};
use crate::folder::FolderPackage;
use crate::hashfs::{Archive, Content, DirListing, EntryKind};
use crate::hashfs_v1::HashFsV1Archive;
use crate::texture::PackedTexture;
use crate::zip::ZipArchive;

/// One package the file system can mount.
#[derive(Debug)]
pub enum Device {
    /// A game archive (HashFS v2), which may hold packed textures.
    V2(Archive),
    /// A mod package in HashFS v1.
    V1(HashFsV1Archive),
    /// A mod package in ZIP.
    Zip(ZipArchive),
    /// A folder on disk, as a Workshop item's unpacked package.
    Folder(FolderPackage),
}

impl Device {
    /// The package file on disk.
    pub fn path(&self) -> &Path {
        match self {
            Device::V2(a) => a.path(),
            Device::V1(a) => a.path(),
            Device::Zip(a) => a.path(),
            Device::Folder(a) => a.path(),
        }
    }

    /// The game archive, if this is one.
    pub fn as_v2(&self) -> Option<&Archive> {
        match self {
            Device::V2(a) => Some(a),
            _ => None,
        }
    }

    /// Whether the device holds the file `path` (leading slash optional).
    pub fn is_file(&self, path: &str) -> bool {
        match self {
            Device::V2(a) => a.entry(path).is_some_and(|e| e.kind == EntryKind::File),
            Device::V1(a) => a.entry(path).is_some_and(|e| !e.is_directory()),
            Device::Zip(a) => a.contains(path),
            Device::Folder(a) => a.is_file(path),
        }
    }

    /// Whether the device holds the directory `path`.
    pub fn is_dir(&self, path: &str) -> bool {
        match self {
            Device::V2(a) => a
                .entry(path)
                .is_some_and(|e| e.kind == EntryKind::Directory),
            Device::V1(a) => a
                .entry(path.trim_end_matches('/'))
                .is_some_and(|e| e.is_directory()),
            Device::Zip(a) => a.list_dir(path).is_ok(),
            Device::Folder(a) => a.is_dir(path),
        }
    }

    /// Reads the file `path`.
    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        match self {
            Device::V2(a) => a.read(path),
            Device::V1(a) => a.read(path),
            Device::Zip(a) => a.read(path),
            Device::Folder(a) => a.read(path),
        }
    }

    /// Lists the directory `path` (`""` is the root).
    pub fn list_dir(&self, path: &str) -> Result<DirListing> {
        match self {
            Device::V2(a) => a.list_dir(path),
            Device::V1(a) => a.list_dir(path),
            Device::Zip(a) => a.list_dir(path),
            Device::Folder(a) => a.list_dir(path),
        }
    }
}

/// A device and the folder of it that is mounted at the game root.
#[derive(Debug)]
struct Mounted {
    device: Device,
    /// `""`, or a folder with a trailing slash (`base/`).
    root: String,
}

impl Mounted {
    /// `path` (leading slash optional) inside the device.
    fn inner(&self, path: &str) -> String {
        format!("{}{}", self.root, path.trim_start_matches('/'))
    }
}

/// Packages in mount order (index 0 mounted first, lowest priority).
#[derive(Debug, Default)]
pub struct Vfs {
    mounted: Vec<Mounted>,
}

impl Vfs {
    /// An empty file system.
    pub fn new() -> Self {
        Self::default()
    }

    /// Mounts the game archive `archive` on top of everything mounted so far.
    pub fn mount(&mut self, archive: Archive) {
        self.mount_device(Device::V2(archive), "");
    }

    /// Mounts `device` on top of everything mounted so far, its folder
    /// `root` (`""` for the whole package, or e.g. `base`) at the game root.
    pub fn mount_device(&mut self, device: Device, root: &str) {
        let root = root.trim_matches('/');
        let root = if root.is_empty() {
            String::new()
        } else {
            format!("{root}/")
        };
        self.mounted.push(Mounted { device, root });
    }

    /// The mounted game archives (HashFS v2), first mounted first.
    pub fn archives(&self) -> impl Iterator<Item = &Archive> {
        self.mounted.iter().filter_map(|m| m.device.as_v2())
    }

    /// Every mounted device and its root folder (`""` or `base/`), first
    /// mounted first.
    pub fn devices(&self) -> impl Iterator<Item = (&Device, &str)> {
        self.mounted.iter().map(|m| (&m.device, m.root.as_str()))
    }

    /// Whether nothing is mounted.
    pub fn is_empty(&self) -> bool {
        self.mounted.is_empty()
    }

    fn top(&self, path: &str) -> Option<&Mounted> {
        self.mounted
            .iter()
            .rev()
            .find(|m| m.device.is_file(&m.inner(path)))
    }

    /// The highest-priority device containing the file `path`.
    pub fn locate(&self, path: &str) -> Option<&Device> {
        self.top(path).map(|m| &m.device)
    }

    /// Whether any device holds the file `path`.
    pub fn exists(&self, path: &str) -> bool {
        self.top(path).is_some()
    }

    /// Reads a plain file from the highest-priority device containing it.
    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        let top = self
            .top(path)
            .ok_or_else(|| ArchiveError::NotFound(path.to_owned()))?;
        top.device.read(&top.inner(path))
    }

    /// Reads a packed texture (addressed by its `.tobj` path). When the
    /// winning `.tobj` is a plain file, as in mod packages, the error is
    /// [`ArchiveError::NotPackedTexture`]: the caller reads the `.tobj` and
    /// the `.dds` it names instead.
    pub fn read_texture(&self, path: &str) -> Result<PackedTexture> {
        let top = self
            .top(path)
            .ok_or_else(|| ArchiveError::NotFound(path.to_owned()))?;
        let inner = top.inner(path);
        match &top.device {
            Device::V2(archive)
                if archive
                    .entry(&inner)
                    .is_some_and(|e| matches!(e.content, Content::Texture { .. })) =>
            {
                archive.read_texture(&inner)
            }
            _ => Err(ArchiveError::NotPackedTexture(path.to_owned())),
        }
    }

    /// Merges the listing of `path` from every device that has it. Names
    /// are sorted and de-duplicated.
    pub fn list_dir(&self, path: &str) -> Result<DirListing> {
        let mut files = BTreeSet::new();
        let mut directories = BTreeSet::new();
        let mut found = false;
        for mounted in &self.mounted {
            let inner = mounted.inner(path);
            let inner = inner.trim_end_matches('/');
            if !mounted.device.is_dir(inner) {
                continue;
            }
            found = true;
            let listing = mounted.device.list_dir(inner)?;
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::zip_write::ZipWriter;

    /// A ZIP package written to a unique temporary file.
    fn zip(files: &[(&str, &[u8])]) -> (ZipArchive, PathBuf) {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let mut writer = ZipWriter::new();
        for (path, bytes) in files {
            writer.add(path, bytes).unwrap();
        }
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("specsheet-vfs-{}-{n}.zip", std::process::id()));
        std::fs::write(&path, writer.finish().unwrap()).unwrap();
        (ZipArchive::open(&path).unwrap(), path)
    }

    #[test]
    fn later_devices_win_and_listings_merge() {
        let (a, pa) = zip(&[("def/a.sii", b"a1"), ("def/both.sii", b"low")]);
        let (b, pb) = zip(&[("def/b.sii", b"b1"), ("def/both.sii", b"high")]);
        let mut vfs = Vfs::new();
        vfs.mount_device(Device::Zip(a), "");
        vfs.mount_device(Device::Zip(b), "");
        assert_eq!(vfs.read("def/both.sii").unwrap(), b"high");
        assert_eq!(vfs.read("/def/a.sii").unwrap(), b"a1");
        assert_eq!(vfs.locate("def/a.sii").unwrap().path(), pa);
        let listing = vfs.list_dir("def").unwrap();
        assert_eq!(listing.files, ["a.sii", "b.sii", "both.sii"]);
        assert_eq!(vfs.list_dir("/").unwrap().directories, ["def"]);
        assert!(matches!(
            vfs.read("def/none.sii"),
            Err(ArchiveError::NotFound(_))
        ));
        assert_eq!(vfs.archives().count(), 0);
        drop(vfs);
        let _ = std::fs::remove_file(pa);
        let _ = std::fs::remove_file(pb);
    }

    /// A multi-folder package (the Sanax Volvo FH2022 definitions): its
    /// `base/` folder is mounted at the game root, and nested folders list
    /// without ZIP directory records.
    #[test]
    fn a_root_folder_is_mounted_at_the_game_root() {
        let (pkg, path) = zip(&[
            ("manifest.sii", b"m"),
            ("base/def/vehicle/truck/x/accessory/s/p.sii", b"part"),
            ("dlc_toys/def/vehicle/truck/x/accessory/t/q.sii", b"toy"),
        ]);
        let mut vfs = Vfs::new();
        vfs.mount_device(Device::Zip(pkg), "base");
        assert_eq!(
            vfs.read("/def/vehicle/truck/x/accessory/s/p.sii").unwrap(),
            b"part"
        );
        assert!(!vfs.exists("manifest.sii"));
        assert!(!vfs.exists("def/vehicle/truck/x/accessory/t/q.sii"));
        assert_eq!(
            vfs.list_dir("def/vehicle/truck/x/accessory")
                .unwrap()
                .directories,
            ["s"]
        );
        assert_eq!(vfs.list_dir("").unwrap().directories, ["def"]);
        drop(vfs);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_loose_tobj_is_not_a_packed_texture() {
        let (pkg, path) = zip(&[("material/x.tobj", b"tobj"), ("material/x.dds", b"DDS ")]);
        let mut vfs = Vfs::new();
        vfs.mount_device(Device::Zip(pkg), "");
        assert!(matches!(
            vfs.read_texture("/material/x.tobj"),
            Err(ArchiveError::NotPackedTexture(p)) if p == "/material/x.tobj"
        ));
        assert!(matches!(
            vfs.read_texture("material/none.tobj"),
            Err(ArchiveError::NotFound(_))
        ));
        drop(vfs);
        let _ = std::fs::remove_file(path);
    }
}
