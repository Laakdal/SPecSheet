//! A profile's mods, found and opened the game's way (D20, F14): the ETS2
//! home folder, its profiles and their `active_mods[]`, each package's
//! container (told by content), its `manifest.sii` version gate, and the
//! folders of multi-folder packages. Steam Workshop items are read from the
//! Workshop folder of the install's Steam library: the entry their
//! `versions.sii` picks for the game version, a folder or a package file
//! (`research/evidence/mod-workshop.txt`). Everything is only read.
//!
//! The devices come back in `active_mods[]` order, index 0 first, so the
//! caller mounts them with rising priority and the later mod wins
//! ([`crate::GameInstall::mount_with_mods`]).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use specsheet_archive::{Archive, Device, FolderPackage, HashFsV1Archive, ZipArchive};
use specsheet_core::glob_match;
use specsheet_formats::{scsc, sii};

use crate::error::{GameDataError, Result};

/// The prefix of a Steam Workshop entry in `active_mods[]`.
const WORKSHOP_PREFIX: &str = "mod_workshop_package.";
/// The root folder of a multi-folder package that is always mounted.
const BASE_FOLDER: &str = "base";

/// The ETS2 home folder: `profiles/`, `mod/`, `game.log.txt`. The game
/// moves it with `-homedir`, which only the user knows, so it can be set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Home {
    root: PathBuf,
}

impl Home {
    /// A home folder at `root`; it must hold `profiles/` or `mod/`.
    pub fn at(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        if !root.join("profiles").is_dir() && !root.join("mod").is_dir() {
            return Err(GameDataError::NotAnInstall {
                path: root,
                reason: "no profiles or mod folder (not an ETS2 home folder)".to_owned(),
            });
        }
        Ok(Self { root })
    }

    /// The default home folder, `Documents/Euro Truck Simulator 2`, if it
    /// exists (also under a OneDrive-redirected Documents folder).
    pub fn discover() -> Option<Self> {
        let profile = PathBuf::from(std::env::var_os("USERPROFILE")?);
        [
            profile.join("Documents"),
            profile.join("OneDrive").join("Documents"),
        ]
        .into_iter()
        .find_map(|docs| Self::at(docs.join("Euro Truck Simulator 2")).ok())
    }

    /// The folder itself.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Every profile that reads, sorted by folder name. A profile that
    /// fails to read is left out.
    pub fn profiles(&self) -> Vec<Profile> {
        let Ok(dirs) = std::fs::read_dir(self.root.join("profiles")) else {
            return Vec::new();
        };
        let mut out: Vec<Profile> = dirs
            .filter_map(|d| d.ok())
            .filter_map(|d| Profile::read(&d.path()).ok())
            .collect();
        out.sort_by(|a, b| a.folder.cmp(&b.folder));
        out
    }

    /// The profile in folder `folder` of `profiles/`.
    pub fn profile(&self, folder: &str) -> Result<Profile> {
        Profile::read(&self.root.join("profiles").join(folder))
    }
}

/// One entry of a profile's `active_mods[]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveMod {
    /// The package name: a file stem in `<home>/mod/`, or a Workshop id.
    pub package: String,
    /// The name the mod manager shows.
    pub display_name: String,
}

impl ActiveMod {
    fn parse(entry: &str) -> Self {
        let (package, display) = entry.split_once('|').unwrap_or((entry, entry));
        Self {
            package: package.to_owned(),
            display_name: display.to_owned(),
        }
    }

    /// Whether this is a Steam Workshop item.
    pub fn is_workshop(&self) -> bool {
        self.package.starts_with(WORKSHOP_PREFIX)
    }

    /// The Workshop item id (`mod_workshop_package.<id in hex>`), which
    /// names its folder in decimal.
    pub fn workshop_id(&self) -> Option<u64> {
        let hex = self.package.strip_prefix(WORKSHOP_PREFIX)?;
        u64::from_str_radix(hex, 16).ok()
    }
}

/// A player profile: its folder, name and active mods.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    /// The folder name under `profiles/` (the name in hex).
    pub folder: String,
    /// The profile's name.
    pub name: String,
    /// `active_mods[]`, index 0 mounted first.
    pub active_mods: Vec<ActiveMod>,
}

impl Profile {
    /// Reads `<dir>/profile.sii` (encrypted or plain).
    pub fn read(dir: &Path) -> Result<Self> {
        let file = dir.join("profile.sii");
        let bytes = std::fs::read(&file).map_err(|source| GameDataError::Io {
            path: file.clone(),
            source,
        })?;
        let definition = |message: String| GameDataError::Definition {
            path: file.display().to_string(),
            message,
        };
        let text = scsc::sii_text(&bytes).map_err(|e| definition(e.to_string()))?;
        let document = sii::parse(&file.display().to_string(), &text)?;
        let unit = document
            .units_of_class("user_profile")
            .next()
            .ok_or_else(|| definition("no user_profile unit".to_owned()))?;
        let folder = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let name = unit
            .get_str("profile_name")
            .map(str::to_owned)
            .or_else(|| hex_name(&folder))
            .unwrap_or_else(|| folder.clone());
        Ok(Self {
            name,
            active_mods: unit
                .array_str("active_mods")
                .into_iter()
                .map(ActiveMod::parse)
                .collect(),
            folder,
        })
    }
}

/// A profile folder name is the profile's name in hex.
fn hex_name(folder: &str) -> Option<String> {
    if !folder.len().is_multiple_of(2) || folder.is_empty() {
        return None;
    }
    let bytes: Option<Vec<u8>> = (0..folder.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(folder.get(i..i + 2)?, 16).ok())
        .collect();
    String::from_utf8(bytes?).ok()
}

/// What became of one active mod.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModStatus {
    /// Mounted; the folders mounted at the game root (`""` for the whole
    /// package, or `base` and installed `dlc_<name>` folders).
    Mounted {
        /// The folders, in mount order.
        folders: Vec<String>,
    },
    /// A Steam Workshop item that is not in the Workshop folder (not
    /// downloaded, or the install has no Workshop folder beside it).
    WorkshopMissing,
    /// No file `<package>.*` in `<home>/mod/`.
    Missing,
    /// Its `compatible_versions[]` matches no glob of the game version.
    Incompatible {
        /// The versions it names.
        versions: Vec<String>,
    },
    /// The file is no package the app reads.
    Unreadable {
        /// Why.
        reason: String,
    },
}

/// One active mod and what became of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModEntry {
    /// The profile's entry.
    pub active: ActiveMod,
    /// The outcome.
    pub status: ModStatus,
}

/// A device to mount and the folder of it that goes to the game root.
#[derive(Debug)]
pub struct ModDevice {
    /// The opened package.
    pub device: Device,
    /// `""` or a folder such as `base`.
    pub root: String,
}

/// Opens the packages of `profile`'s active mods, in `active_mods[]`
/// order: local ones from `<home>/mod/`, Workshop items from `workshop`
/// (`GameInstall::workshop_dir`). `game_version` gates by
/// `compatible_versions[]` and picks a Workshop item's entry, and a
/// multi-folder package's `dlc_<name>` folder is mounted only when
/// `installed_dlcs` holds `dlc_<name>`.
pub fn open_active_mods(
    home: &Home,
    profile: &Profile,
    game_version: &str,
    installed_dlcs: &BTreeSet<String>,
    workshop: Option<&Path>,
) -> (Vec<ModDevice>, Vec<ModEntry>) {
    let packages = package_files(&home.root.join("mod"));
    let mut devices = Vec::new();
    let mut report = Vec::new();
    for active in &profile.active_mods {
        let status = if active.is_workshop() {
            let item = workshop
                .zip(active.workshop_id())
                .map(|(dir, id)| dir.join(id.to_string()))
                .filter(|dir| dir.is_dir());
            match item {
                None => ModStatus::WorkshopMissing,
                Some(item) => open_workshop_item(&item, game_version, installed_dlcs, &mut devices),
            }
        } else {
            match find_package(&packages, &active.package) {
                None => ModStatus::Missing,
                Some(file) => open_package(file, game_version, installed_dlcs, &mut devices),
            }
        };
        report.push(ModEntry {
            active: active.clone(),
            status,
        });
    }
    (devices, report)
}

/// The files directly in the mod folder.
fn package_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    // `DirEntry::file_type` does not follow links, so a link named like a
    // package is not opened.
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .collect();
    files.sort();
    files
}

/// The file whose stem is `package` (exactly, else ignoring case as
/// Windows does).
fn find_package<'a>(files: &'a [PathBuf], package: &str) -> Option<&'a PathBuf> {
    let stem = |p: &PathBuf| p.file_stem().map(|s| s.to_string_lossy().into_owned());
    files
        .iter()
        .find(|p| stem(p).as_deref() == Some(package))
        .or_else(|| {
            files
                .iter()
                .find(|p| stem(p).is_some_and(|s| s.eq_ignore_ascii_case(package)))
        })
}

/// Opens a package by its content, as the game does (`SCS#` → HashFS by
/// version, `PK` → ZIP), never by extension.
fn open_device(file: &Path) -> std::result::Result<Device, String> {
    let mut head = [0u8; 6];
    let read = std::fs::File::open(file)
        .and_then(|mut f| std::io::Read::read(&mut f, &mut head))
        .map_err(|e| e.to_string())?;
    let head = &head[..read];
    if head.starts_with(b"SCS#") {
        match head.get(4) {
            Some(1) => HashFsV1Archive::open(file).map(Device::V1),
            _ => Archive::open(file).map(Device::V2),
        }
        .map_err(|e| e.to_string())
    } else if head.starts_with(b"PK") {
        ZipArchive::open(file)
            .map(Device::Zip)
            .map_err(|e| e.to_string())
    } else {
        Err("neither a HashFS nor a ZIP package".to_owned())
    }
}

fn open_package(
    file: &Path,
    game_version: &str,
    installed_dlcs: &BTreeSet<String>,
    devices: &mut Vec<ModDevice>,
) -> ModStatus {
    mount_package(&|| open_device(file), game_version, installed_dlcs, devices)
}

/// One `package_version_info` of a Workshop item's `versions.sii`.
struct PackageVersion {
    /// A folder of the item, or the stem of a package file in it.
    name: String,
    /// Game version globs; none means any version.
    versions: Vec<String>,
}

/// The `package_version_info` units of `<item>/versions.sii`, in file order.
fn package_versions(item: &Path) -> std::result::Result<Vec<PackageVersion>, String> {
    let file = item.join("versions.sii");
    let bytes = std::fs::read(&file).map_err(|e| format!("versions.sii: {e}"))?;
    let text = scsc::sii_text(&bytes).map_err(|e| format!("versions.sii: {e}"))?;
    let document = sii::parse("/versions.sii", &text).map_err(|e| e.to_string())?;
    Ok(document
        .units_of_class("package_version_info")
        .filter_map(|unit| {
            Some(PackageVersion {
                name: unit.get_str("package_name")?.to_owned(),
                versions: unit
                    .array_str("compatible_versions")
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            })
        })
        .collect())
}

/// The entry the game mounts for `game_version`: the first unit whose
/// `compatible_versions` match it, else the first unit without any.
/// 77 of 77 Workshop items in the test install's game log agree
/// (`research/evidence/mod-workshop.txt`; Confirmed on data).
fn pick_version<'a>(units: &'a [PackageVersion], game_version: &str) -> Option<&'a str> {
    let matching = units
        .iter()
        .find(|u| !u.versions.is_empty() && u.versions.iter().any(|v| glob_match(v, game_version)));
    matching
        .or_else(|| units.iter().find(|u| u.versions.is_empty()))
        .map(|u| u.name.as_str())
}

/// Opens a Workshop item: the entry its `versions.sii` picks, a folder or
/// a package file whose stem is the name. A folder holding `base.*` and
/// `dlc_<name>.*` packages mounts those after itself, `base` first and the
/// installed DLCs by name, as the game log shows for item 695738881.
fn open_workshop_item(
    item: &Path,
    game_version: &str,
    installed_dlcs: &BTreeSet<String>,
    devices: &mut Vec<ModDevice>,
) -> ModStatus {
    let units = match package_versions(item) {
        Ok(units) => units,
        Err(reason) => return ModStatus::Unreadable { reason },
    };
    let Some(name) = pick_version(&units, game_version) else {
        return ModStatus::Incompatible {
            versions: units.into_iter().flat_map(|u| u.versions).collect(),
        };
    };
    let not_held = || ModStatus::Unreadable {
        reason: format!("versions.sii names {name:?}, which is no entry of the item"),
    };
    // `package_name` is hostile input: one plain name, never a path.
    if !is_plain_name(name) {
        return not_held();
    }
    let Some(folder) = folder_inside(item, name) else {
        let packages: Vec<PathBuf> = package_files(item)
            .into_iter()
            .filter(|p| is_package_file(p))
            .collect();
        return match find_package(&packages, name) {
            Some(file) => open_package(file, game_version, installed_dlcs, devices),
            None => not_held(),
        };
    };
    let open = || {
        FolderPackage::open(&folder)
            .map(Device::Folder)
            .map_err(|e| e.to_string())
    };
    // Devices are handed over only when the whole item opens.
    let mut opened = Vec::new();
    let status = mount_package(&open, game_version, installed_dlcs, &mut opened);
    let ModStatus::Mounted { mut folders } = status else {
        return status;
    };
    for file in inner_packages(&folder, installed_dlcs) {
        match open_device(&file) {
            Ok(device) => {
                opened.push(ModDevice {
                    device,
                    root: String::new(),
                });
                folders.extend(file.file_name().map(|n| n.to_string_lossy().into_owned()));
            }
            Err(reason) => return ModStatus::Unreadable { reason },
        }
    }
    devices.extend(opened);
    ModStatus::Mounted { folders }
}

/// A single file or folder name: not empty, not `.` or `..`, and without a
/// path or drive separator.
fn is_plain_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':', '\0'])
}

/// The folder `name` of `item`, when it is a real folder (not a link) whose
/// resolved path stays inside the item's.
fn folder_inside(item: &Path, name: &str) -> Option<PathBuf> {
    let folder = item.join(name);
    let kind = std::fs::symlink_metadata(&folder).ok()?.file_type();
    if !kind.is_dir() {
        return None;
    }
    let real = std::fs::canonicalize(&folder).ok()?;
    let item = std::fs::canonicalize(item).ok()?;
    real.starts_with(&item).then_some(real)
}

/// A `.zip` or `.scs` file, the package files a Workshop item holds.
fn is_package_file(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip") || e.eq_ignore_ascii_case("scs"))
}

/// The `base.*` and installed `dlc_<name>.*` package files directly in a
/// Workshop item's folder, `base` first, then the DLCs by name.
fn inner_packages(folder: &Path, installed_dlcs: &BTreeSet<String>) -> Vec<PathBuf> {
    let stem = |p: &PathBuf| {
        p.file_stem()
            .map(|s| s.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default()
    };
    let files: Vec<PathBuf> = package_files(folder)
        .into_iter()
        .filter(|p| is_package_file(p))
        .collect();
    let mut out: Vec<PathBuf> = files
        .iter()
        .filter(|p| stem(p) == BASE_FOLDER)
        .cloned()
        .collect();
    let mut dlcs: Vec<PathBuf> = files
        .into_iter()
        .filter(|p| stem(p).starts_with("dlc_") && installed_dlcs.contains(&stem(p)))
        .collect();
    dlcs.sort_by_key(stem);
    out.extend(dlcs);
    out
}

/// Gates a package by its manifest and mounts it, or each folder of a
/// multi-folder package as its own device; `open` opens it again for
/// each further folder.
fn mount_package(
    open: &dyn Fn() -> std::result::Result<Device, String>,
    game_version: &str,
    installed_dlcs: &BTreeSet<String>,
    devices: &mut Vec<ModDevice>,
) -> ModStatus {
    let device = match open() {
        Ok(device) => device,
        Err(reason) => return ModStatus::Unreadable { reason },
    };
    let versions = compatible_versions(&device);
    if !versions.is_empty() && !versions.iter().any(|v| glob_match(v, game_version)) {
        return ModStatus::Incompatible { versions };
    }
    let Some(folders) = mounted_folders(&device, installed_dlcs) else {
        devices.push(ModDevice {
            device,
            root: String::new(),
        });
        return ModStatus::Mounted {
            folders: vec![String::new()],
        };
    };
    // One device per folder (F14: each folder is its own device); the
    // first reuses the opened package, the others open it again. Nothing
    // is handed over unless every folder opens.
    let mut first = Some(device);
    let mut opened = Vec::new();
    for folder in &folders {
        let device = match first.take() {
            Some(device) => device,
            None => match open() {
                Ok(device) => device,
                Err(reason) => return ModStatus::Unreadable { reason },
            },
        };
        opened.push(ModDevice {
            device,
            root: folder.clone(),
        });
    }
    devices.extend(opened);
    ModStatus::Mounted { folders }
}

/// The `compatible_versions[]` of the package's `manifest.sii`; empty when
/// there is no manifest or no array (compatible).
fn compatible_versions(device: &Device) -> Vec<String> {
    let Ok(bytes) = device.read("manifest.sii") else {
        return Vec::new();
    };
    let Ok(document) = sii::parse("/manifest.sii", &bytes) else {
        return Vec::new();
    };
    let unit = document.units_of_class("mod_package").next();
    unit.map(|unit| {
        unit.array_str("compatible_versions")
            .into_iter()
            .map(str::to_owned)
            .collect()
    })
    .unwrap_or_default()
}

/// The folders of a multi-folder package to mount, `base` first when there
/// is one, then the installed `dlc_<name>` folders by name; `None` for an
/// ordinary package (no `base/` and no `dlc_*/` folder at its root). A
/// package may hold `dlc_*` folders and no `base/` (promods-eu-dlcsupport,
/// `research/evidence/mod-multi-folder.txt`), and may then mount none.
/// Which test the game uses is a Hypothesis (F14).
fn mounted_folders(device: &Device, installed_dlcs: &BTreeSet<String>) -> Option<Vec<String>> {
    let has_base = device.is_dir(BASE_FOLDER);
    let mut dlcs: Vec<String> = device
        .list_dir("")
        .map(|root| root.directories)
        .unwrap_or_default()
        .into_iter()
        .filter(|d| d.starts_with("dlc_"))
        .collect();
    if !has_base && dlcs.is_empty() {
        return None;
    }
    dlcs.retain(|d| installed_dlcs.contains(d));
    dlcs.sort();
    let base = has_base.then(|| BASE_FOLDER.to_owned());
    Some(base.into_iter().chain(dlcs).collect())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use specsheet_archive::ZipWriter;

    use super::*;

    struct TempHome(PathBuf);

    impl TempHome {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let root =
                std::env::temp_dir().join(format!("specsheet-home-{}-{n}", std::process::id()));
            std::fs::create_dir_all(root.join("mod")).unwrap();
            std::fs::create_dir_all(root.join("profiles")).unwrap();
            Self(root)
        }

        fn package(&self, file: &str, files: &[(&str, &[u8])]) {
            let mut writer = ZipWriter::new();
            for (path, bytes) in files {
                writer.add(path, bytes).unwrap();
            }
            std::fs::write(self.0.join("mod").join(file), writer.finish().unwrap()).unwrap();
        }

        fn profile(&self, folder: &str, text: &str) {
            let dir = self.0.join("profiles").join(folder);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("profile.sii"), text).unwrap();
        }
    }

    impl Drop for TempHome {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const MANIFEST_160: &[u8] =
        b"SiiNunit\n{\nmod_package : .m {\n compatible_versions[]: \"1.60.*\"\n}\n}\n";
    const MANIFEST_149: &[u8] =
        b"SiiNunit\n{\nmod_package : .m {\n compatible_versions[]: \"1.49.*\"\n}\n}\n";

    #[test]
    fn active_mods_open_in_order_with_their_outcomes() {
        let home = TempHome::new();
        home.package("plain.scs", &[("def/a.sii", b"a")]);
        home.package(
            "multi.zip",
            &[
                ("manifest.sii", MANIFEST_160),
                ("base/def/b.sii", b"b"),
                ("dlc_toys/def/c.sii", b"c"),
                ("dlc_absent/def/d.sii", b"d"),
            ],
        );
        home.package("old.scs", &[("manifest.sii", MANIFEST_149)]);
        // dlc_* folders and no base/ (promods-eu-dlcsupport): only the
        // installed DLCs' folders mount (mod-multi-folder.txt section 1).
        home.package(
            "dlconly.zip",
            &[("dlc_toys/def/e.sii", b"e"), ("dlc_absent/def/f.sii", b"f")],
        );
        std::fs::write(home.0.join("mod").join("junk.scs"), b"nothing").unwrap();
        // "Blahaj" in hex, as the game names profile folders.
        home.profile(
            "426c6168616a",
            "SiiNunit\n{\nuser_profile : .p {\n active_mods: 7\n \
             active_mods[0]: \"mod_workshop_package.00000000AEDD308C|Ws\"\n \
             active_mods[1]: \"plain|Plain mod\"\n \
             active_mods[2]: \"Multi|Multi\"\n \
             active_mods[3]: \"old|Old\"\n \
             active_mods[4]: \"junk|Junk\"\n \
             active_mods[5]: \"gone|Gone\"\n \
             active_mods[6]: \"dlconly|Dlc only\"\n}\n}\n",
        );
        let home_dir = Home::at(&home.0).unwrap();
        let profiles = home_dir.profiles();
        assert_eq!(profiles.len(), 1);
        let profile = &profiles[0];
        assert_eq!(profile.name, "Blahaj");
        assert_eq!(profile.active_mods[1].display_name, "Plain mod");

        let dlcs = BTreeSet::from(["dlc_toys".to_owned()]);
        let (devices, report) = open_active_mods(&home_dir, profile, "1.60.1.7", &dlcs, None);
        let statuses: Vec<&ModStatus> = report.iter().map(|e| &e.status).collect();
        assert_eq!(statuses[0], &ModStatus::WorkshopMissing);
        assert_eq!(
            statuses[1],
            &ModStatus::Mounted {
                folders: vec![String::new()]
            }
        );
        // Found ignoring case; only the installed DLC's folder mounts.
        assert_eq!(
            statuses[2],
            &ModStatus::Mounted {
                folders: vec!["base".to_owned(), "dlc_toys".to_owned()]
            }
        );
        assert_eq!(
            statuses[3],
            &ModStatus::Incompatible {
                versions: vec!["1.49.*".to_owned()]
            }
        );
        assert!(matches!(statuses[4], ModStatus::Unreadable { .. }));
        assert_eq!(statuses[5], &ModStatus::Missing);
        assert_eq!(
            statuses[6],
            &ModStatus::Mounted {
                folders: vec!["dlc_toys".to_owned()]
            }
        );
        let roots: Vec<&str> = devices.iter().map(|d| d.root.as_str()).collect();
        assert_eq!(roots, ["", "base", "dlc_toys", "dlc_toys"]);
    }

    /// A Workshop item under `<workshop>/<id in decimal>/`, its files
    /// given as (path, bytes).
    fn workshop_item(root: &Path, id: u64, files: &[(&str, &[u8])]) {
        let item = root.join(id.to_string());
        for (path, bytes) in files {
            let target = item.join(path);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::write(target, bytes).unwrap();
        }
    }

    fn zip_bytes(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = ZipWriter::new();
        for (path, bytes) in files {
            writer.add(path, bytes).unwrap();
        }
        writer.finish().unwrap()
    }

    /// D20: a Workshop item mounts the entry its `versions.sii` picks: the
    /// first unit whose `compatible_versions` match the game, else the
    /// first without any (77 of 77 items in the game log agree,
    /// `research/evidence/mod-workshop.txt`). The entry is a folder or a
    /// package file; a folder holding `base.*` and `dlc_*.*` packages
    /// mounts those after itself, installed DLCs only.
    #[test]
    fn workshop_items_mount_the_entry_their_versions_pick() {
        let home = TempHome::new();
        let workshop = home.0.join("workshop");
        let versions = |units: &str| format!("SiiNunit\n{{\n{units}}}\n").into_bytes();
        let latest_first = versions(
            "package_version_info : .latest { package_name: \"latest\" }\n\
             package_version_info : .v159 { package_name: \"old\" compatible_versions[]: \"1.59.*\" }\n",
        );
        workshop_item(
            &workshop,
            10,
            &[
                ("versions.sii", &latest_first),
                ("latest/manifest.sii", MANIFEST_160),
                ("latest/def/w.sii", b"folder"),
                ("old/def/w.sii", b"too old"),
            ],
        );
        let matching_wins = versions(
            "package_version_info : .any { package_name: \"universal\" }\n\
             package_version_info : .v160 { package_name: \"pack\" compatible_versions[]: \"1.60.*\" }\n",
        );
        let pack = zip_bytes(&[("def/p.sii", b"zip")]);
        workshop_item(
            &workshop,
            11,
            &[
                ("versions.sii", &matching_wins),
                ("universal/def/p.sii", b"not picked"),
                ("pack.zip", &pack),
            ],
        );
        let base = zip_bytes(&[("def/b.sii", b"base")]);
        let toys = zip_bytes(&[("def/t.sii", b"toys")]);
        workshop_item(
            &workshop,
            12,
            &[
                (
                    "versions.sii",
                    &versions("package_version_info : .u { package_name: \"tp\" }\n"),
                ),
                ("tp/manifest.sii", MANIFEST_160),
                ("tp/base.zip", &base),
                ("tp/dlc_toys.zip", &toys),
                ("tp/dlc_absent.zip", &toys),
            ],
        );
        let only_old = versions(
            "package_version_info : .v149 { package_name: \"x\" compatible_versions[]: \"1.49.*\" }\n",
        );
        workshop_item(&workshop, 13, &[("versions.sii", &only_old)]);
        home.profile(
            "70",
            "SiiNunit\n{\nuser_profile : .p {\n active_mods: 5\n \
             active_mods[0]: \"mod_workshop_package.000000000000000A|Folder\"\n \
             active_mods[1]: \"mod_workshop_package.000000000000000B|Zip\"\n \
             active_mods[2]: \"mod_workshop_package.000000000000000C|Archives\"\n \
             active_mods[3]: \"mod_workshop_package.000000000000000D|Old\"\n \
             active_mods[4]: \"mod_workshop_package.000000000000000E|Gone\"\n}\n}\n",
        );
        let home_dir = Home::at(&home.0).unwrap();
        let profile = home_dir.profile("70").unwrap();
        assert_eq!(profile.active_mods[0].workshop_id(), Some(10));
        let dlcs = BTreeSet::from(["dlc_toys".to_owned()]);
        let (devices, report) =
            open_active_mods(&home_dir, &profile, "1.60.1.7", &dlcs, Some(&workshop));
        let statuses: Vec<&ModStatus> = report.iter().map(|e| &e.status).collect();
        assert_eq!(
            statuses[..3],
            [
                &ModStatus::Mounted {
                    folders: vec![String::new()]
                },
                &ModStatus::Mounted {
                    folders: vec![String::new()]
                },
                &ModStatus::Mounted {
                    folders: vec![
                        String::new(),
                        "base.zip".to_owned(),
                        "dlc_toys.zip".to_owned()
                    ]
                },
            ]
        );
        assert_eq!(
            statuses[3],
            &ModStatus::Incompatible {
                versions: vec!["1.49.*".to_owned()]
            }
        );
        assert_eq!(statuses[4], &ModStatus::WorkshopMissing);

        let mut vfs = specsheet_archive::Vfs::new();
        for device in devices {
            vfs.mount_device(device.device, &device.root);
        }
        assert_eq!(vfs.read("def/w.sii").unwrap(), b"folder");
        assert_eq!(vfs.read("def/p.sii").unwrap(), b"zip");
        assert_eq!(vfs.read("def/b.sii").unwrap(), b"base");
        assert_eq!(vfs.read("def/t.sii").unwrap(), b"toys");

        // Without a Workshop folder every item is missing.
        let (_, report) = open_active_mods(&home_dir, &profile, "1.60.1.7", &dlcs, None);
        assert!(report
            .iter()
            .all(|e| e.status == ModStatus::WorkshopMissing));
    }

    /// A hostile `versions.sii` never mounts anything outside its item, and
    /// an item that fails part way mounts nothing (review of D20).
    #[test]
    fn workshop_items_stay_inside_their_folder_and_fail_whole() {
        let home = TempHome::new();
        let workshop = home.0.join("workshop");
        let outside = home.0.join("outside");
        std::fs::create_dir_all(outside.join("def")).unwrap();
        std::fs::write(outside.join("def/x.sii"), b"outside").unwrap();
        let versions = |name: &str| {
            format!("SiiNunit\n{{\npackage_version_info : .u {{ package_name: \"{name}\" }}\n}}\n")
                .into_bytes()
        };
        let absolute = outside.display().to_string().replace('\\', "/");
        let escapes = [
            versions("../../outside"),
            versions(&absolute),
            versions("sub/../../../outside"),
            versions(""),
            versions("."),
        ];
        for (n, bytes) in escapes.iter().enumerate() {
            workshop_item(&workshop, 20 + n as u64, &[("versions.sii", bytes)]);
        }
        // A folder with good content and a broken DLC package beside it,
        // and an archive the item lists under another extension.
        let good = zip_bytes(&[("def/g.sii", b"g")]);
        workshop_item(
            &workshop,
            30,
            &[
                ("versions.sii", &versions("tp")),
                ("tp/def/x.sii", b"inside"),
                ("tp/base.zip", &good),
                ("tp/dlc_toys.zip", b"not a package"),
            ],
        );
        workshop_item(
            &workshop,
            31,
            &[
                ("versions.sii", &versions("pack")),
                ("pack.7z", b"not ours"),
                ("pack.zip", &good),
            ],
        );
        let mut entries = String::new();
        let ids: Vec<u64> = (20..25).chain([30, 31]).collect();
        for (i, id) in ids.iter().enumerate() {
            entries.push_str(&format!(
                " active_mods[{i}]: \"mod_workshop_package.{id:016X}|W{id}\"\n"
            ));
        }
        home.profile(
            "71",
            &format!(
                "SiiNunit\n{{\nuser_profile : .p {{\n active_mods: {}\n{entries}}}\n}}\n",
                ids.len()
            ),
        );
        let home_dir = Home::at(&home.0).unwrap();
        let profile = home_dir.profile("71").unwrap();
        let dlcs = BTreeSet::from(["dlc_toys".to_owned()]);
        let (devices, report) =
            open_active_mods(&home_dir, &profile, "1.60.1.7", &dlcs, Some(&workshop));
        for entry in &report[..6] {
            assert!(
                matches!(entry.status, ModStatus::Unreadable { .. }),
                "{}: {:?}",
                entry.active.display_name,
                entry.status
            );
        }
        // The .7z beside the .zip is not a package; the .zip mounts.
        assert_eq!(
            report[6].status,
            ModStatus::Mounted {
                folders: vec![String::new()]
            }
        );
        assert_eq!(devices.len(), 1, "only item 31 mounts");
        let mut vfs = specsheet_archive::Vfs::new();
        for device in devices {
            vfs.mount_device(device.device, &device.root);
        }
        assert!(!vfs.exists("def/x.sii"));
        assert_eq!(vfs.read("def/g.sii").unwrap(), b"g");
    }

    #[test]
    fn a_folder_without_profiles_or_mods_is_not_a_home() {
        let dir = std::env::temp_dir().join(format!("specsheet-nohome-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(Home::at(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn profile_folders_are_hex_names() {
        assert_eq!(hex_name("426c6168616a").as_deref(), Some("Blahaj"));
        assert_eq!(hex_name("4"), None);
        assert_eq!(hex_name("zz"), None);
    }
}
