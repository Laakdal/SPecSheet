//! Shows what a mod package (HashFS v1 or ZIP) holds, in the terminal: a
//! folder tree with file counts, sizes and how many files replace a file of
//! the game, or with `--files` every file. File names of a v1 package are
//! recovered first (`mod_names`), with every path of the mounted game as
//! candidates and, with `--others <dir>`, the paths that the other packages
//! in `<dir>` mention (cached per package in
//! `%LOCALAPPDATA%\dev.specsheet.app\mod-scan` unless `--no-cache`). Files
//! without a recovered name are shown as `_unnamed/<hash>.<extension>`, the
//! extension guessed from their content. `--filter` keeps paths matching a
//! pattern (`*` any characters, `?` one). The package is only read.
//!
//! Usage (ETS2_DIR set, or the install is discovered):
//!   cargo run --release -p specsheet-research --example mod_view -- [--files] [--depth <n>] [--filter <pattern>] [--others <dir>] [--no-cache] <package>

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use specsheet_archive::{HashFsV1Archive, ZipArchive};
use specsheet_data::mod_names::{game_paths, learn_word_swaps, recover_v1_names};
use specsheet_data::mod_scan::folder_candidates;
use specsheet_data::GameInstall;
use specsheet_formats::sniff;

/// Folder levels shown when `--depth` is not given.
const DEFAULT_DEPTH: usize = 3;

/// One file to show.
struct Shown {
    path: String,
    size: u64,
    /// The game has a file at this path, so the package replaces it.
    game: bool,
    named: bool,
}

/// Where the scan of `--others` is cached: the app's local data folder.
fn cache_dir() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    Some(Path::new(&local).join("dev.specsheet.app").join("mod-scan"))
}

/// Whether `text` matches `pattern`, where `*` stands for any characters
/// (`/` included) and `?` for one.
fn matches(pattern: &[u8], text: &[u8]) -> bool {
    let (mut p, mut t) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p].eq_ignore_ascii_case(&text[t])) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            mark = t;
            p += 1;
        } else if let Some(s) = star {
            p = s + 1;
            mark += 1;
            t = mark;
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&c| c == b'*')
}

fn size(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 30 => format!("{:.1} GB", b as f64 / f64::from(1 << 30)),
        b if b >= 1 << 20 => format!("{:.1} MB", b as f64 / f64::from(1 << 20)),
        b if b >= 1 << 10 => format!("{:.1} KB", b as f64 / f64::from(1 << 10)),
        b => format!("{b} B"),
    }
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
    let files = flag("--files", false).is_some();
    let depth = flag("--depth", true).map_or(Ok(DEFAULT_DEPTH), |d| d.parse())?;
    let filter = flag("--filter", true);
    let others = flag("--others", true);
    let no_cache = flag("--no-cache", false).is_some();
    let [package] = <[String; 1]>::try_from(args).map_err(|_| {
        "usage: mod_view [--files] [--depth <n>] [--filter <pattern>] [--others <dir>] [--no-cache] <package>"
    })?;

    let (vfs, _) = GameInstall::discover()?.mount()?;
    let game = game_paths(&vfs);
    let game_set: HashSet<&str> = game.iter().map(String::as_str).collect();

    let (kind, mut shown) = if let Ok(zip) = ZipArchive::open(&package) {
        let shown = zip
            .files()
            .map(|path| Shown {
                path: path.to_owned(),
                size: zip.size(path).unwrap_or(0),
                game: game_set.contains(path),
                named: true,
            })
            .collect::<Vec<_>>();
        ("ZIP", shown)
    } else {
        let archive = HashFsV1Archive::open(&package)?;
        let mut candidates = game.clone();
        let swaps = learn_word_swaps(&candidates);
        if let Some(dir) = &others {
            let cache = if no_cache { None } else { cache_dir() };
            candidates.extend(folder_candidates(Path::new(dir), cache.as_deref())?.paths);
        }
        let names = recover_v1_names(&archive, candidates, &swaps);
        let mut shown = Vec::new();
        for entry in archive.entries().filter(|e| !e.is_directory()) {
            let (path, named) = match names.names.get(&entry.hash) {
                Some(path) => (path.clone(), true),
                None => {
                    let ext = archive
                        .read_entry(entry)
                        .map_or("bin", |bytes| sniff::extension(&bytes));
                    (format!("_unnamed/{:016x}.{ext}", entry.hash), false)
                }
            };
            shown.push(Shown {
                game: game_set.contains(path.as_str()),
                path,
                size: u64::from(entry.size),
                named,
            });
        }
        ("HashFS v1", shown)
    };
    let total = shown.len();
    let named = shown.iter().filter(|s| s.named).count();
    if let Some(pattern) = &filter {
        shown.retain(|s| {
            matches(
                pattern.trim_start_matches('/').as_bytes(),
                s.path.as_bytes(),
            )
        });
    }
    shown.sort_by(|a, b| a.path.cmp(&b.path));

    let name = Path::new(&package)
        .file_name()
        .map_or(package.clone(), |n| n.to_string_lossy().into_owned());
    let bytes: u64 = shown.iter().map(|s| s.size).sum();
    let replaces = shown.iter().filter(|s| s.game).count();
    println!(
        "{name}: {kind}, {total} files ({named} named){}",
        filter
            .as_ref()
            .map_or(String::new(), |f| format!(", {} matching {f}", shown.len()))
    );
    println!(
        "{} files, {}, {replaces} replace game files",
        shown.len(),
        size(bytes)
    );
    println!();

    if files {
        for s in &shown {
            let mark = if s.game { "[game]" } else { "" };
            println!("{:>10}  {mark:<6}  {}", size(s.size), s.path);
        }
        return Ok(());
    }
    // Folder totals down to `depth` levels; deeper files count in their
    // folder at that depth.
    let mut folders: BTreeMap<String, (usize, u64, usize)> = BTreeMap::new();
    for s in &shown {
        let parts: Vec<&str> = s.path.split('/').collect();
        let levels = (parts.len() - 1).min(depth);
        for level in 0..=levels {
            let folder = if level == 0 {
                String::new()
            } else {
                parts[..level].join("/") + "/"
            };
            let entry = folders.entry(folder).or_default();
            entry.0 += 1;
            entry.1 += s.size;
            entry.2 += usize::from(s.game);
        }
    }
    for (folder, (count, bytes, game)) in &folders {
        let level = folder.matches('/').count();
        let label = if folder.is_empty() {
            "/".to_owned()
        } else {
            folder
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or(folder)
                .to_owned()
                + "/"
        };
        let game = if *game > 0 {
            format!("{game} replace game files")
        } else {
            String::new()
        };
        println!(
            "{:<52} {count:>6} {:<5} {:>10}  {game}",
            format!("{}{label}", "  ".repeat(level)),
            if *count == 1 { "file" } else { "files" },
            size(*bytes)
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn patterns_match_like_a_shell_glob() {
        assert!(matches(b"*.sii", b"def/a/b.sii"));
        assert!(matches(
            b"def/*/sound.sui",
            b"def/vehicle/truck/x/sound.sui"
        ));
        assert!(matches(b"a?c", b"abc"));
        assert!(matches(b"*SCANIA*", b"def/scania.s_2016/x"));
        assert!(!matches(b"*.sii", b"def/a/b.sui"));
        assert!(!matches(b"a?c", b"ac"));
    }
}
