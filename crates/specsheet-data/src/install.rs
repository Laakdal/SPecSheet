//! Locating an ETS2 installation and mounting its archives in game order.

use std::path::{Path, PathBuf};

use specsheet_archive::{Archive, Vfs};
use specsheet_formats::sii;

use crate::error::{GameDataError, Result};

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

    pub fn root(&self) -> &Path {
        &self.root
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

    /// Archive paths in mount order: the base archives, then every
    /// `dlc_*.scs` sorted by name. Mods are not mounted.
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

    /// Opens and mounts every archive. Archives that fail to open are
    /// reported in the returned list instead of aborting the whole mount.
    pub fn mount(&self) -> Result<(Vfs, Vec<(PathBuf, specsheet_archive::ArchiveError)>)> {
        let mut vfs = Vfs::new();
        let mut failures = Vec::new();
        for path in self.archive_paths()? {
            match Archive::open(&path) {
                Ok(archive) => vfs.mount(archive),
                Err(error) => failures.push((path, error)),
            }
        }
        if vfs.archives().is_empty() {
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

    #[test]
    fn rejects_directories_without_game_archives() {
        let dir = std::env::temp_dir();
        assert!(matches!(
            GameInstall::at(&dir),
            Err(GameDataError::NotAnInstall { .. })
        ));
    }
}
