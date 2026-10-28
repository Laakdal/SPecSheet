//! Research tool: recovers the file names of HashFS v1 mod packages
//! (`mod_names`), first from the package alone, then with every path of the
//! mounted game as candidates. With `--others <dir>`, every other package in
//! `<dir>` adds candidates: a ZIP's file names, and the paths that a ZIP's or
//! a v1 package's files mention (a v1 package may hold the models that
//! another package's definitions use). With `--paths <file>`, each line of
//! `<file>` is a candidate too. Packages are only read.
//!
//! Usage (ETS2_DIR set, or the install is discovered):
//!   cargo run --release -p specsheet-research --example mod_names -- [--list] [--others <dir>] [--paths <file>] <package.scs> ...

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use specsheet_archive::{HashFsV1Archive, ZipArchive};
use specsheet_data::mod_names::{game_paths, recover_v1_names};
use specsheet_data::GameInstall;
use specsheet_formats::paths::referenced_paths;

/// Small text-like files whose paths are worth scanning.
const SCANNED: &[&str] = &[".sii", ".sui", ".mat", ".tobj", ".pmd", ".soundref"];
/// v1 entries larger than this are not scanned (as in `mod_names`).
const MAX_SCAN: u32 = 1 << 20;

/// Candidates from every package in `dir` (the named packages' own
/// references are found anyway, so they need not be skipped).
fn other_packages(dir: &str) -> std::io::Result<Vec<String>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if let Ok(zip) = ZipArchive::open(&path) {
            for file in zip.files() {
                out.push(file.to_owned());
                if SCANNED.iter().any(|ext| file.ends_with(ext)) {
                    if let Ok(bytes) = zip.read(file) {
                        out.extend(referenced_paths(file, &bytes));
                    }
                }
            }
        } else if let Ok(v1) = HashFsV1Archive::open(&path) {
            for entry in v1.entries() {
                if entry.is_directory() || entry.size > MAX_SCAN {
                    continue;
                }
                if let Ok(bytes) = v1.read_entry(entry) {
                    out.extend(referenced_paths("", &bytes));
                }
            }
        }
    }
    Ok(out)
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
    let extra = flag("--paths", true);
    let packages = args;

    let mut extra_paths = Vec::new();
    if let Some(dir) = &others {
        extra_paths.extend(other_packages(dir)?);
        println!("candidates from other packages: {}", extra_paths.len());
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
        let alone = recover_v1_names(&archive, Vec::new());
        let with_game = recover_v1_names(&archive, game.iter().cloned());
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
