//! Extracts a mod package (HashFS v1 or ZIP) or a game archive (HashFS v2)
//! to a folder, or lists it as a tree (D19, `specsheet_data::extract`); the
//! container is told by its header. A game archive's names come from its
//! listings, then from every path of the mounted game (`v2_names`); its
//! packed textures are left out and listed, or with `--textures` written as
//! a standalone `.tobj` and a DX10 `.dds` each (`extract::loose_files`).
//! File names of a v1 package are
//! recovered first (`mod_names`), with every path of the mounted game as
//! candidates and, with `--others <dir>`, the paths that the other packages
//! in `<dir>` mention, cached per package in
//! `%LOCALAPPDATA%\dev.specsheet.app\mod-scan` unless `--no-cache`. Word
//! swaps learned from the game's file names add variants of found names.
//!
//! `--list` prints the files as a tree, named as an extraction would name
//! them, and writes nothing. `--only <pattern>` (repeatable) takes only the
//! entries whose archive path matches: `*` matches any run of characters,
//! `/` included, and case is ignored (`def/*`, `*.dds`). Unnamed entries
//! match as `_unnamed/<hash>`, e.g. `--only "_unnamed/*"`.
//!
//! The destination must be absolute and may not overlap the game install,
//! the ETS2 home folder (`ETS2_HOME`, and `Documents\Euro Truck Simulator
//! 2`), the folder holding the package, or this repository. Nothing is overwritten, and an extraction that would write
//! more than `--max-gb` (default 64 GiB) of the files taken is refused.
//!
//! Usage (ETS2_DIR set, or the install is discovered):
//!   cargo run --release -p specsheet-research --example mod_extract -- --list [--only <pattern>]... [--others <dir>] [--no-cache] <package>
//!   cargo run --release -p specsheet-research --example mod_extract -- [--dry-run] [--skip-existing] [--max-gb <n>] [--textures] [--only <pattern>]... [--others <dir>] [--no-cache] <package> <destination>

use std::path::{Path, PathBuf};

use specsheet_archive::{Archive as GameArchive, HashFsV1Archive, ZipArchive};
use specsheet_data::extract::{
    extract, list, v2_names, Archive, ExtractOptions, Listing, PathFilter, V2Names,
};
use specsheet_data::mod_names::{game_paths, learn_word_swaps, recover_v1_names, RecoveredNames};
use specsheet_data::mod_scan::folder_candidates;
use specsheet_data::GameInstall;

const USAGE: &str = "usage: mod_extract --list [--only <pattern>]... [--others <dir>] [--no-cache] <package>\n   or: mod_extract [--dry-run] [--skip-existing] [--max-gb <n>] [--textures] [--only <pattern>]... [--others <dir>] [--no-cache] <package> <destination>";

/// Prints the usage and exits.
fn usage() -> ! {
    eprintln!("{USAGE}");
    std::process::exit(2)
}

/// Where the scan of `--others` is cached: the app's local data folder.
fn cache_dir() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    Some(Path::new(&local).join("dev.specsheet.app").join("mod-scan"))
}

/// Which reader a file needs, from its first bytes: `SCS#` and the HashFS
/// version, else ZIP (which the game finds by content, not extension).
fn hashfs_version(path: &Path) -> std::io::Result<Option<u16>> {
    use std::io::Read;
    let mut head = [0u8; 6];
    let mut file = std::fs::File::open(path)?;
    let n = file.read(&mut head)?;
    Ok((n == head.len() && &head[..4] == b"SCS#").then(|| u16::from_le_bytes([head[4], head[5]])))
}

/// What `main` does once the package is open.
enum Task {
    List,
    Extract {
        dest: PathBuf,
        options: ExtractOptions,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut flag = |name: &str, with_value: bool| -> Option<String> {
        let at = args.iter().position(|a| a == name)?;
        args.remove(at);
        if !with_value {
            return Some(String::new());
        }
        if at >= args.len() {
            usage();
        }
        Some(args.remove(at))
    };
    let listing = flag("--list", false).is_some();
    let textures = flag("--textures", false).is_some();
    let mut options = ExtractOptions {
        dry_run: flag("--dry-run", false).is_some(),
        skip_existing: flag("--skip-existing", false).is_some(),
        ..ExtractOptions::default()
    };
    if let Some(gb) = flag("--max-gb", true) {
        options.max_bytes = gb.parse::<u64>()?.saturating_mul(1 << 30);
    }
    let mut patterns = Vec::new();
    while let Some(pattern) = flag("--only", true) {
        patterns.push(pattern);
    }
    let filter = PathFilter::new(&patterns);
    let others = flag("--others", true);
    let no_cache = flag("--no-cache", false).is_some();
    let (package, task) = match (listing, args.as_slice()) {
        (true, [package]) => (PathBuf::from(package), Task::List),
        (false, [package, dest]) => (
            PathBuf::from(package),
            Task::Extract {
                dest: PathBuf::from(dest),
                options,
            },
        ),
        _ => usage(),
    };

    let install = GameInstall::discover()?;
    let mut protected = vec![
        install.root().to_path_buf(),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."),
    ];
    if let Some(folder) = package.parent() {
        protected.push(folder.to_path_buf());
    }
    if let Some(home) = std::env::var_os("ETS2_HOME") {
        protected.push(PathBuf::from(home));
    }
    if let Some(home) = std::env::var_os("USERPROFILE") {
        protected.push(
            Path::new(&home)
                .join("Documents")
                .join("Euro Truck Simulator 2"),
        );
    }

    let zip;
    let v1;
    let v2;
    let names: RecoveredNames;
    let game_names: V2Names;
    let archive = match hashfs_version(&package)? {
        None => {
            zip = ZipArchive::open(&package)?;
            Archive::Zip(&zip)
        }
        Some(2) => {
            v2 = GameArchive::open(&package)?;
            let (vfs, _) = install.mount()?;
            game_names = v2_names(&v2, &game_paths(&vfs));
            println!(
                "{}: {} entries, {} named by listings, {} by game paths, {} unnamed, {} not indexed",
                package.display(),
                v2.len(),
                game_names.from_listings,
                game_names.from_candidates,
                game_names.unnamed.len(),
                v2.skipped_entries().len()
            );
            Archive::V2 {
                archive: &v2,
                names: &game_names,
                textures,
            }
        }
        Some(_) => {
            v1 = HashFsV1Archive::open(&package)?;
            let (vfs, _) = install.mount()?;
            let mut candidates = game_paths(&vfs);
            let swaps = learn_word_swaps(&candidates);
            if let Some(dir) = &others {
                let cache = if no_cache { None } else { cache_dir() };
                let scan = folder_candidates(Path::new(dir), cache.as_deref())?;
                println!(
                    "other packages: {} read, {} from cache",
                    scan.scanned, scan.cached
                );
                candidates.extend(scan.paths);
            }
            names = recover_v1_names(&v1, candidates, &swaps);
            println!(
                "{}: {} entries, {} named, {} unnamed",
                package.display(),
                v1.len(),
                names.names.len(),
                names.unnamed.len()
            );
            Archive::V1 {
                archive: &v1,
                names: &names,
            }
        }
    };

    let (dest, options) = match task {
        Task::List => {
            print_listing(&list(&archive, &filter));
            return Ok(());
        }
        Task::Extract { dest, options } => (dest, options),
    };
    let done = extract(&archive, &dest, &protected, options, &filter)?;
    let verb = if options.dry_run {
        "would write"
    } else {
        "wrote"
    };
    println!(
        "{verb} {} files ({:.1} MB), {} under _unnamed/, {} renamed, {} skipped, {} unreadable, {} packed textures left out",
        done.written,
        done.bytes as f64 / 1e6,
        done.unnamed,
        done.renamed.len(),
        done.skipped,
        done.unreadable.len(),
        done.packed_textures.len()
    );
    for (from, to) in done.renamed.iter().take(10) {
        println!("    renamed {from} -> {to}");
    }
    Ok(())
}

/// Prints the files of `listing` as an indented folder tree with sizes,
/// noting renamed and unnamed entries, then a total and the entries left
/// out.
fn print_listing(listing: &Listing) {
    let mut files: Vec<_> = listing.files.iter().collect();
    files.sort_by(|a, b| a.path.split('/').cmp(b.path.split('/')));
    let mut open: Vec<&str> = Vec::new();
    for file in &files {
        let parts: Vec<&str> = file.path.split('/').collect();
        let Some((name, dirs)) = parts.split_last() else {
            continue;
        };
        let common = open.iter().zip(dirs).take_while(|(a, b)| *a == *b).count();
        open.truncate(common);
        for dir in &dirs[common..] {
            println!("{}{dir}/", "  ".repeat(open.len()));
            open.push(dir);
        }
        let note = match &file.source {
            None => "  (unnamed)".to_owned(),
            Some(source) if source.trim_start_matches('/') != file.path => {
                format!("  <- {source}")
            }
            Some(_) => String::new(),
        };
        println!(
            "{}{name}  {}{note}",
            "  ".repeat(open.len()),
            human(file.size)
        );
    }
    let bytes: u64 = files.iter().map(|f| f.size).sum();
    let unnamed = files.iter().filter(|f| f.source.is_none()).count();
    println!(
        "{} files ({}), {unnamed} unnamed",
        files.len(),
        human(bytes)
    );
    if !listing.packed_textures.is_empty() || !listing.unreadable.is_empty() {
        println!(
            "left out: {} packed textures, {} entries not indexed",
            listing.packed_textures.len(),
            listing.unreadable.len()
        );
    }
}

/// `bytes` with a unit, for reading.
fn human(bytes: u64) -> String {
    match bytes {
        0..1_000 => format!("{bytes} B"),
        1_000..1_000_000 => format!("{:.1} kB", bytes as f64 / 1e3),
        1_000_000..1_000_000_000 => format!("{:.1} MB", bytes as f64 / 1e6),
        _ => format!("{:.2} GB", bytes as f64 / 1e9),
    }
}
