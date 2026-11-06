//! Checks a folder that `mod_extract` wrote from a game archive (HashFS v2)
//! against the game, file by file (D19, `research/evidence/`). Each file
//! the archive's listing names is compared with the mounted game's bytes
//! (`Vfs::read`) where this archive is the one the game reads the path
//! from, and with the archive's own bytes where a higher archive overrides
//! it. Unnamed files are compared with the archive entry their hash names.
//! Nothing is written.
//!
//! Usage (ETS2_DIR set, or the install is discovered):
//!   cargo run --release -p specsheet-research --example extract_check -- <archive.scs> <extracted folder>

use std::fs;
use std::path::{Path, PathBuf};

use specsheet_archive::Archive as GameArchive;
use specsheet_data::extract::{list, v2_names, Archive, PathFilter};
use specsheet_data::mod_names::game_paths;
use specsheet_data::GameInstall;

/// How the files compared.
#[derive(Default)]
struct Tally {
    /// Equal to `Vfs::read` (this archive is the top one for the path).
    game_same: usize,
    game_differs: usize,
    /// Overridden by a higher archive: equal to this archive's own bytes.
    own_same: usize,
    own_differs: usize,
    /// Overridden paths whose bytes also equal the game's.
    overridden_but_equal: usize,
    /// Unnamed: equal to the entry their hash names.
    unnamed_same: usize,
    unnamed_differs: usize,
    /// Listed but not on disk, or not readable from the archive.
    missing: Vec<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [archive_path, folder] = <[String; 2]>::try_from(args)
        .map_err(|_| "usage: extract_check <archive.scs> <extracted folder>")?;
    let folder = PathBuf::from(folder);
    let install = GameInstall::discover()?;
    let (vfs, _) = install.mount()?;
    let archive = GameArchive::open(&archive_path)?;
    let me = fs::canonicalize(&archive_path)?;
    let names = v2_names(&archive, &game_paths(&vfs));
    let listing = list(
        &Archive::V2 {
            archive: &archive,
            names: &names,
        },
        &PathFilter::default(),
    );

    let mut tally = Tally::default();
    for file in &listing.files {
        let on_disk = match &file.source {
            Some(_) => fs::read(folder.join(&file.path)).ok(),
            None => unnamed_on_disk(&folder, &file.path),
        };
        let Some(on_disk) = on_disk else {
            tally.missing.push(file.path.clone());
            continue;
        };
        let Some(path) = &file.source else {
            let own = hash_of(&file.path)
                .and_then(|hash| archive.entry_by_hash(hash))
                .and_then(|entry| archive.read_entry(entry).ok());
            match own {
                Some(own) if own == on_disk => tally.unnamed_same += 1,
                Some(_) => tally.unnamed_differs += 1,
                None => tally.missing.push(file.path.clone()),
            }
            continue;
        };
        let top = vfs
            .locate(path)
            .and_then(|a| fs::canonicalize(a.path()).ok());
        if top.as_deref() == Some(me.as_path()) {
            match vfs.read(path) {
                Ok(game) if game == on_disk => tally.game_same += 1,
                Ok(_) => tally.game_differs += 1,
                Err(_) => tally.missing.push(path.clone()),
            }
        } else {
            match archive.read(path) {
                Ok(own) if own == on_disk => tally.own_same += 1,
                Ok(_) => tally.own_differs += 1,
                Err(_) => tally.missing.push(path.clone()),
            }
            if vfs.read(path).is_ok_and(|game| game == on_disk) {
                tally.overridden_but_equal += 1;
            }
        }
    }
    let on_disk = count_files(&folder);
    let lists = ["_renamed.txt", "_unreadable.txt", "_packed_textures.txt"]
        .iter()
        .filter(|name| folder.join(name).is_file())
        .count();

    println!("{archive_path}");
    println!(
        "  listed files {}, files on disk {} ({} of them lists), packed textures left out {}, not indexed {}",
        listing.files.len(),
        on_disk,
        lists,
        listing.packed_textures.len(),
        listing.unreadable.len()
    );
    println!(
        "  top archive for the path, vs Vfs::read: {} equal, {} differ",
        tally.game_same, tally.game_differs
    );
    println!(
        "  overridden by a higher archive, vs this archive's read: {} equal, {} differ ({} also equal the game's bytes)",
        tally.own_same, tally.own_differs, tally.overridden_but_equal
    );
    println!(
        "  unnamed, vs the entry of their hash: {} equal, {} differ",
        tally.unnamed_same, tally.unnamed_differs
    );
    println!("  missing or unreadable: {}", tally.missing.len());
    for path in tally.missing.iter().take(10) {
        println!("    {path}");
    }
    Ok(())
}

/// The bytes of the unnamed file `_unnamed/<hash>.bin` as written, whatever
/// extension extraction gave it.
fn unnamed_on_disk(folder: &Path, listed: &str) -> Option<Vec<u8>> {
    let (dir, name) = listed.rsplit_once('/')?;
    let stem = name.split('.').next()?;
    let dir = folder.join(dir);
    let found = fs::read_dir(&dir).ok()?.flatten().find(|e| {
        e.file_name()
            .to_str()
            .is_some_and(|n| n.split('.').next() == Some(stem))
    })?;
    fs::read(found.path()).ok()
}

/// The entry hash in an unnamed file's name.
fn hash_of(listed: &str) -> Option<u64> {
    let name = listed.rsplit('/').next()?;
    u64::from_str_radix(name.split('.').next()?, 16).ok()
}

fn count_files(folder: &Path) -> usize {
    let mut count = 0;
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                count += 1;
            }
        }
    }
    count
}
