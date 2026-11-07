//! Locating an ETS2 installation and mounting its archives in game order.

use std::path::{Path, PathBuf};

use std::collections::BTreeSet;

use specsheet_archive::{Archive, Device, Vfs};
use specsheet_formats::sii;

use crate::error::{GameDataError, Result};
use crate::mods::ModDevice;

/// The game version that the research, docs and real-install tests were
/// checked against. Game updates can change formats and definitions, so the
/// real-install tests fail on any other version until someone rechecks.
pub const TESTED_GAME_VERSION: &str = "1.60.1.7";

/// Steam application id of Euro Truck Simulator 2.
const ETS2_APP_ID: &str = "227300";
const GAME_DIR_NAME: &str = "Euro Truck Simulator 2";

/// Base archives in the order the game mounts them, taken from the game's
/// own `game.log.txt` (see `docs/REVERSE_ENGINEERING.md#mount-order`).
pub const BASE_MOUNT_ORDER: &[&str] = &[
    "effect.scs",
    "base.scs",
    "base_map.scs",
    "base_vehicle.scs",
    "base_share.scs",
    "def.scs",
    "core.scs",
    "locale.scs",
    "base_cfg.scs",
];

/// The pool an archive mounts into. Lookups ask the core pool, then the mod
/// pool, then the scs pool (F2, F14, Confirmed static,
/// `research/evidence/mod-pool-priority-static.txt`); the variants are in
/// rising lookup rank.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Pool {
    Scs,
    Mod,
    Core,
}

/// The first mod's priority; each later active mod's device ranks higher
/// (F14, `research/evidence/mod-priority-static.txt`).
const FIRST_MOD_PRIORITY: i64 = 1001;

/// A mounted file system and the archives that failed to open (reported,
/// not fatal).
pub type Mounted = (Vfs, Vec<(PathBuf, specsheet_archive::ArchiveError)>);

/// Where an archive ranks in lookups: by pool, then by mount priority
/// (higher wins). Archives of equal rank keep their mount order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct MountRank {
    pool: Pool,
    priority: i64,
}

/// The game's rank for the archive `name` (F2, Confirmed static): effect
/// 452 and core, locale, base_cfg 100000 in the core pool; def 51, a DLC its
/// manifest's `relative_priority` + 100 (0 when it has none) and every other
/// archive (the base ones) 50 in the scs pool.
fn mount_rank(name: &str, relative_priority: Option<i64>) -> MountRank {
    let (pool, priority) = match name {
        "effect.scs" => (Pool::Core, 452),
        "core.scs" | "locale.scs" | "base_cfg.scs" => (Pool::Core, 100_000),
        "def.scs" => (Pool::Scs, 51),
        _ if name.starts_with("dlc_") => (Pool::Scs, relative_priority.unwrap_or(0) + 100),
        _ => (Pool::Scs, 50),
    };
    MountRank { pool, priority }
}

/// The `relative_priority` of a DLC manifest (`dlc_package`), if it has one.
fn relative_priority(manifest: &[u8]) -> Option<i64> {
    let document = sii::parse("/dlc.manifest.sii", manifest).ok()?;
    let unit = document.units_of_class("dlc_package").next()?;
    unit.get_i64("relative_priority")
}

/// A validated ETS2 installation directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameInstall {
    root: PathBuf,
}

impl GameInstall {
    /// Validates that `root` looks like an ETS2 installation.
    pub fn at(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        let missing: Vec<_> = ["base.scs", "def.scs"]
            .into_iter()
            .filter(|name| !root.join(name).is_file())
            .collect();
        if !missing.is_empty() {
            return Err(GameDataError::NotAnInstall {
                path: root,
                reason: format!("missing {}", missing.join(", ")),
            });
        }
        Ok(Self { root })
    }

    /// Finds the installation: `ETS2_DIR` if set, otherwise the Steam
    /// library folders of the default Steam installation.
    pub fn discover() -> Result<Self> {
        if let Some(dir) = std::env::var_os("ETS2_DIR") {
            return Self::at(PathBuf::from(dir));
        }
        steam_library_candidates()
            .into_iter()
            .map(|library| library.join("steamapps").join("common").join(GAME_DIR_NAME))
            .find_map(|candidate| Self::at(candidate).ok())
            .ok_or(GameDataError::InstallNotFound)
    }

    /// The game directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The Steam Workshop folder of the Steam library holding the game,
    /// `<library>/steamapps/workshop/content/227300`, if it exists. Only
    /// read (D20).
    pub fn workshop_dir(&self) -> Option<PathBuf> {
        let steamapps = self.root.parent()?.parent()?;
        let dir = steamapps.join("workshop").join("content").join(ETS2_APP_ID);
        dir.is_dir().then_some(dir)
    }

    /// The installed game version, e.g. `"1.60.1.7"`, read from the
    /// `fs_pack_set` unit in `version.scs/version.sii`.
    pub fn version(&self) -> Result<String> {
        let archive = Archive::open(self.root.join("version.scs"))?;
        let bytes = archive.read("version.sii")?;
        let document = sii::parse("/version.sii", &bytes)?;
        let version = document
            .units_of_class("fs_pack_set")
            .find_map(|unit| unit.get_str("version"))
            .map(str::to_owned);
        version.ok_or_else(|| GameDataError::Definition {
            path: "version.scs/version.sii".to_owned(),
            message: "no fs_pack_set version".to_owned(),
        })
    }

    /// The archives to mount: the base archives in the game's log order, then
    /// every `dlc_*.scs` sorted by name. [`GameInstall::mount`] puts them in
    /// lookup order.
    pub fn archive_paths(&self) -> Result<Vec<PathBuf>> {
        let mut paths: Vec<PathBuf> = BASE_MOUNT_ORDER
            .iter()
            .map(|name| self.root.join(name))
            .filter(|path| path.is_file())
            .collect();
        let mut dlcs: Vec<PathBuf> = std::fs::read_dir(&self.root)
            .map_err(|source| GameDataError::Io {
                path: self.root.clone(),
                source,
            })?
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|path| {
                path.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("dlc_") && n.ends_with(".scs"))
            })
            .collect();
        dlcs.sort();
        paths.extend(dlcs);
        Ok(paths)
    }

    /// Opens and mounts every archive in the game's lookup order: the scs
    /// pool below the core pool, each by priority, a DLC by the
    /// `relative_priority` in its own `/<name>.manifest.sii` (F2). Archives
    /// that fail to open are reported in the returned list instead of
    /// aborting the whole mount.
    pub fn mount(&self) -> Result<Mounted> {
        self.mount_with_mods(Vec::new())
    }

    /// The installed DLCs, by archive stem (`dlc_toys`).
    pub fn installed_dlcs(&self) -> Result<BTreeSet<String>> {
        Ok(self
            .archive_paths()?
            .iter()
            .filter_map(|p| p.file_stem()?.to_str().map(str::to_owned))
            .filter(|stem| stem.starts_with("dlc_"))
            .collect())
    }

    /// As [`GameInstall::mount`], with `mods` (in `active_mods[]` order,
    /// [`crate::mods::open_active_mods`]) in the mod pool: above every base,
    /// def and DLC archive, below the core pool, the later device winning
    /// (D20, F14).
    pub fn mount_with_mods(&self, mods: Vec<ModDevice>) -> Result<Mounted> {
        let mut opened = Vec::new();
        let mut failures = Vec::new();
        for path in self.archive_paths()? {
            match Archive::open(&path) {
                Ok(archive) => {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let priority = name
                        .strip_suffix(".scs")
                        .filter(|stem| stem.starts_with("dlc_"))
                        .and_then(|stem| archive.read(&format!("/{stem}.manifest.sii")).ok())
                        .and_then(|bytes| relative_priority(&bytes));
                    opened.push((
                        mount_rank(&name, priority),
                        Device::V2(archive),
                        String::new(),
                    ));
                }
                Err(error) => failures.push((path, error)),
            }
        }
        for (index, m) in mods.into_iter().enumerate() {
            let rank = MountRank {
                pool: Pool::Mod,
                priority: FIRST_MOD_PRIORITY.saturating_add(index as i64),
            };
            opened.push((rank, m.device, m.root));
        }
        // A stable sort: archives of equal rank keep the order above.
        opened.sort_by_key(|(rank, _, _)| *rank);
        let mut vfs = Vfs::new();
        for (_, device, root) in opened {
            vfs.mount_device(device, &root);
        }
        if vfs.is_empty() {
            return Err(GameDataError::NotAnInstall {
                path: self.root.clone(),
                reason: "no archive could be opened".to_owned(),
            });
        }
        Ok((vfs, failures))
    }
}

/// Library folder names commonly created on secondary drives.
const DRIVE_LIBRARY_NAMES: &[&str] = &["Steam Library", "SteamLibrary", "Steam", "Games\\Steam"];

/// Steam library roots listed in `libraryfolders.vdf`, the Steam root, and
/// common library folders on every drive (the VDF is not always current).
fn steam_library_candidates() -> Vec<PathBuf> {
    let steam_roots = [
        PathBuf::from(r"C:\Program Files (x86)\Steam"),
        PathBuf::from(r"C:\Program Files\Steam"),
    ];
    let mut libraries = Vec::new();
    for root in steam_roots {
        let vdf = root.join("steamapps").join("libraryfolders.vdf");
        if let Ok(text) = std::fs::read_to_string(&vdf) {
            libraries.extend(parse_library_folders(&text));
        }
        libraries.push(root);
    }
    for drive in b'C'..=b'Z' {
        for name in DRIVE_LIBRARY_NAMES {
            let candidate = PathBuf::from(format!("{}:\\{name}", drive as char));
            if candidate.is_dir() {
                libraries.push(candidate);
            }
        }
    }
    libraries
}

/// Library paths from Steam's `libraryfolders.vdf`, libraries that list
/// ETS2 first. Every library is returned because the app list is not always
/// up to date (e.g. after moving an install between libraries).
pub fn parse_library_folders(vdf: &str) -> Vec<PathBuf> {
    let mut with_ets2 = Vec::new();
    let mut others = Vec::new();
    let mut current: Option<(PathBuf, bool)> = None;
    let mut finish = |entry: Option<(PathBuf, bool)>| match entry {
        Some((path, true)) => with_ets2.push(path),
        Some((path, false)) => others.push(path),
        None => {}
    };
    for line in vdf.lines() {
        let fields: Vec<&str> = line.split('"').skip(1).step_by(2).collect();
        match fields.as_slice() {
            ["path", value] => {
                finish(current.take());
                current = Some((PathBuf::from(value.replace("\\\\", "\\")), false));
            }
            [app, _] if *app == ETS2_APP_ID => {
                if let Some((_, has_ets2)) = current.as_mut() {
                    *has_ets2 = true;
                }
            }
            _ => {}
        }
    }
    finish(current);
    with_ets2.extend(others);
    with_ets2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_folders_list_libraries_containing_ets2_first() {
        let vdf = r#""libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"apps"
		{
			"228980"		"304863845"
		}
	}
	"1"
	{
		"path"		"D:\\Steam Library"
		"apps"
		{
			"227300"		"44949470831"
		}
	}
}"#;
        assert_eq!(
            parse_library_folders(vdf),
            [
                PathBuf::from(r"D:\Steam Library"),
                PathBuf::from(r"C:\Program Files (x86)\Steam")
            ]
        );
    }

    /// research/evidence/mod-pool-priority-static.txt: the core pool is
    /// asked before the scs pool, and inside a pool the higher priority
    /// wins. DLCs rank by their manifest's `relative_priority` + 100, not by
    /// name, and stay below the core pool.
    #[test]
    fn archives_rank_by_pool_then_priority() {
        let names = [
            "base.scs",
            "def.scs",
            "core.scs",
            "effect.scs",
            "dlc_a.scs",
            "dlc_b.scs",
        ];
        let priority = |name: &str| match name {
            "dlc_a.scs" => Some(60),
            "dlc_b.scs" => Some(5),
            _ => None,
        };
        let mut ranked: Vec<_> = names
            .iter()
            .map(|n| (mount_rank(n, priority(n)), *n))
            .collect();
        ranked.sort_by_key(|(rank, _)| *rank);
        let order: Vec<_> = ranked.iter().map(|(_, n)| *n).collect();
        assert_eq!(
            order,
            [
                "base.scs",
                "def.scs",
                "dlc_b.scs",
                "dlc_a.scs",
                "effect.scs",
                "core.scs"
            ]
        );
        assert_eq!(
            mount_rank("dlc_b.scs", Some(5)),
            MountRank {
                pool: Pool::Scs,
                priority: 105
            }
        );
        assert_eq!(
            mount_rank("locale.scs", None),
            MountRank {
                pool: Pool::Core,
                priority: 100_000
            }
        );
    }

    /// F14: mods rank above every scs-pool archive, below the core pool,
    /// and the later active mod wins.
    #[test]
    fn the_mod_pool_sits_between_the_scs_and_core_pools() {
        let first_mod = MountRank {
            pool: Pool::Mod,
            priority: FIRST_MOD_PRIORITY,
        };
        let later_mod = MountRank {
            pool: Pool::Mod,
            priority: FIRST_MOD_PRIORITY + 1,
        };
        assert!(mount_rank("dlc_x.scs", Some(400)) < first_mod);
        assert!(first_mod < later_mod);
        assert!(later_mod < mount_rank("effect.scs", None));
    }

    #[test]
    fn reads_a_dlcs_relative_priority_from_its_manifest() {
        let manifest = b"SiiNunit\n{\ndlc_package : _nameless.dlc_east {\n\tid: eut2_east\n\trelative_priority: 1\n}\n}\n";
        assert_eq!(relative_priority(manifest), Some(1));
        assert_eq!(relative_priority(b"SiiNunit\n{\n}\n"), None);
        assert_eq!(relative_priority(b"not sii"), None);
    }

    #[test]
    fn rejects_directories_without_game_archives() {
        let dir = std::env::temp_dir();
        assert!(matches!(
            GameInstall::at(&dir),
            Err(GameDataError::NotAnInstall { .. })
        ));
    }
}
