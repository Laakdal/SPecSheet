//! Research tool: recovers the file names of HashFS v1 mod packages
//! (`mod_names`), first from the package alone, then with every path of the
//! mounted game as candidates. With `--others <dir>`, every other package in
//! `<dir>` adds candidates: a ZIP's file names, and the paths that a ZIP's or
//! a v1 package's files mention (a v1 package may hold the models that
//! another package's definitions use); the scan is cached per package in
//! `%LOCALAPPDATA%\dev.specsheet.app\mod-scan` unless `--no-cache`. With
//! `--paths <file>`, each line of `<file>` is a candidate too. Word swaps
//! learned from the game's file names (`learn_word_swaps`) add variants of
//! found names. Packages are only read.
//!
//! Usage (ETS2_DIR set, or the install is discovered):
//!   cargo run --release -p specsheet-research --example mod_names -- [--list] [--others <dir>] [--no-cache] [--paths <file>] <package.scs> ...

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use specsheet_archive::HashFsV1Archive;
use specsheet_data::mod_names::{game_paths, learn_word_swaps, recover_v1_names, WordSwaps};
use specsheet_data::mod_scan::folder_candidates;
use specsheet_data::GameInstall;

/// Where the scan of `--others` is cached: the app's local data folder.
fn cache_dir() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    Some(Path::new(&local).join("dev.specsheet.app").join("mod-scan"))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut flag = |name: &str, with_value: bool| -> Option<String> {
        let at = args.iter().position(|a| a == name)?;
        args.remove(at);
        Some(if with_value {
            args.remove(at)
        } else {
            String::new()
        })
    };
    let list = flag("--list", false).is_some();
    let others = flag("--others", true);
    let no_cache = flag("--no-cache", false).is_some();
    let extra = flag("--paths", true);
    let packages = args;

    let mut extra_paths = Vec::new();
    if let Some(dir) = &others {
        let start = std::time::Instant::now();
        let cache = if no_cache { None } else { cache_dir() };
        let scan = folder_candidates(Path::new(dir), cache.as_deref())?;
        println!(
            "candidates from other packages: {} ({} packages read, {} from cache, {:?})",
            scan.paths.len(),
            scan.scanned,
            scan.cached,
            start.elapsed()
        );
        extra_paths.extend(scan.paths);
    }
    if let Some(file) = &extra {
        let text = std::fs::read_to_string(file)?;
        let before = extra_paths.len();
        extra_paths.extend(
            text.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_owned),
        );
        println!("candidates from {file}: {}", extra_paths.len() - before);
    }
    let (vfs, _) = GameInstall::discover()?.mount()?;
    let start = std::time::Instant::now();
    let mut game = game_paths(&vfs);
    let game_set: HashSet<String> = game.iter().cloned().collect();
    let swaps = learn_word_swaps(&game);
    println!("game paths: {} ({:?})", game.len(), start.elapsed());
    game.extend(extra_paths);

    let (mut total, mut before, mut after) = (0, 0, 0);
    for path in &packages {
        let archive = match HashFsV1Archive::open(path) {
            Ok(archive) => archive,
            Err(e) => {
                println!("{path}: not read as HashFS v1: {e}");
                continue;
            }
        };
        let alone = recover_v1_names(&archive, Vec::new(), &WordSwaps::default());
        let with_game = recover_v1_names(&archive, game.iter().cloned(), &swaps);
        let overrides = with_game
            .names
            .values()
            .filter(|p| game_set.contains(*p))
            .count();
        let name = Path::new(path)
            .file_name()
            .map_or(path.clone(), |n| n.to_string_lossy().into_owned());
        println!(
            "{name}: {} entries; named {} from the package alone, {} with candidates ({} of them game paths), {} unnamed",
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
    println!("all: {total} entries; named {before} from packages alone, {after} with candidates");
    Ok(())
}
