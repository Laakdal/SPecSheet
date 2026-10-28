//! Research tool: recovers the file names of HashFS v1 mod packages
//! (`mod_names`), first from the package alone, then with every path of the
//! mounted game as candidates. With `--zips <dir>`, the file names of every
//! ZIP package in `<dir>` and the paths their definition files mention are
//! candidates too (a v1 package may hold the models that a ZIP's
//! definitions use). Packages are only read.
//!
//! Usage (ETS2_DIR set, or the install is discovered):
//!   cargo run --release -p specsheet-research --example mod_names -- [--list] [--zips <dir>] <package.scs> ...

use std::collections::{BTreeMap, HashSet};

use specsheet_archive::{HashFsV1Archive, ZipArchive};
use specsheet_data::mod_names::{game_paths, recover_v1_names};
use specsheet_data::GameInstall;
use specsheet_formats::paths::referenced_paths;

/// Small text-like files whose paths are worth scanning.
const SCANNED: &[&str] = &[".sii", ".sui", ".mat", ".tobj", ".pmd", ".soundref"];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let list = args.first().is_some_and(|a| a == "--list");
    if list {
        args.remove(0);
    }
    let mut zip_paths = Vec::new();
    if args.first().is_some_and(|a| a == "--zips") {
        args.remove(0);
        let dir = args.remove(0);
        for entry in std::fs::read_dir(&dir)? {
            let Ok(zip) = ZipArchive::open(entry?.path()) else {
                continue;
            };
            for file in zip.files() {
                zip_paths.push(file.to_owned());
                if SCANNED.iter().any(|ext| file.ends_with(ext)) {
                    if let Ok(bytes) = zip.read(file) {
                        zip_paths.extend(referenced_paths(file, &bytes));
                    }
                }
            }
        }
        println!("ZIP candidates: {}", zip_paths.len());
    }
    let (vfs, _) = GameInstall::discover()?.mount()?;
    let start = std::time::Instant::now();
    let mut game = game_paths(&vfs);
    let game_only = game.len();
    let game_set: HashSet<String> = game.iter().cloned().collect();
    println!("game paths: {game_only} ({:?})", start.elapsed());
    game.extend(zip_paths);

    let (mut total, mut before, mut after) = (0, 0, 0);
    for path in &args {
        let archive = match HashFsV1Archive::open(path) {
            Ok(archive) => archive,
            Err(e) => {
                println!("{path}: not read as HashFS v1: {e}");
                continue;
            }
        };
        let alone = recover_v1_names(&archive, Vec::new());
        let with_game = recover_v1_names(&archive, game.iter().cloned());
        let overrides = with_game
            .names
            .values()
            .filter(|p| game_set.contains(*p))
            .count();
        let name = std::path::Path::new(path)
            .file_name()
            .map_or(path.clone(), |n| n.to_string_lossy().into_owned());
        println!(
            "{name}: {} entries; named {} from the package alone, {} with game paths ({} of them game paths), {} unnamed",
            archive.len(),
            alone.names.len(),
            with_game.names.len(),
            overrides,
            with_game.unnamed.len()
        );
        total += archive.len();
        before += alone.names.len();
        after += with_game.names.len();
        if list {
            let mut dirs: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
            for p in with_game.names.values() {
                let dir = p.rsplit_once('/').map_or("", |(d, _)| d);
                let slot = dirs.entry(dir).or_default();
                slot.0 += 1;
                if game_set.contains(p) {
                    slot.1 += 1;
                }
            }
            for (dir, (n, game)) in dirs {
                println!("    /{dir}: {n} named, {game} also in the game");
            }
            for p in with_game.names.values() {
                if game_set.contains(p) {
                    println!("    replaces /{p}");
                }
            }
            // What the unnamed entries hold: their first printable text.
            let mut heads: BTreeMap<String, usize> = BTreeMap::new();
            for entry in archive.entries() {
                if !with_game.unnamed.contains(&entry.hash) {
                    continue;
                }
                let bytes = archive.read_entry(entry).unwrap_or_default();
                let text: String = String::from_utf8_lossy(&bytes[..bytes.len().min(400)])
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .take(2)
                    .collect::<Vec<_>>()
                    .join(" | ");
                let head: String = text.chars().take(110).collect();
                *heads.entry(head).or_default() += 1;
            }
            for (head, n) in heads {
                println!("    unnamed x{n}: {head}");
            }
        }
    }
    println!("all: {total} entries; named {before} from packages alone, {after} with game paths");
    Ok(())
}
