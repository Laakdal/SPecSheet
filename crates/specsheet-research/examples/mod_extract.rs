//! Extracts a mod package (HashFS v1 or ZIP) to a folder (D19,
//! `specsheet_data::extract`). File names of a v1 package are recovered
//! first (`mod_names`), with every path of the mounted game as candidates
//! and, with `--others <dir>`, the paths that the other packages in `<dir>`
//! mention, cached per package in `%LOCALAPPDATA%\dev.specsheet.app\mod-scan`
//! unless `--no-cache`. Word swaps learned from the game's file names add
//! variants of found names. The destination must be absolute and may not
//! overlap the game install, the ETS2 home folder, the folder holding the
//! package, or this repository. Nothing is overwritten, and an archive that would write
//! more than `--max-gb` (default 64 GiB) is refused.
//!
//! Usage (ETS2_DIR set, or the install is discovered):
//!   cargo run --release -p specsheet-research --example mod_extract -- [--dry-run] [--skip-existing] [--max-gb <n>] [--others <dir>] [--no-cache] <package> <destination>

use std::path::{Path, PathBuf};

use specsheet_archive::{HashFsV1Archive, ZipArchive};
use specsheet_data::extract::{extract, Archive, ExtractOptions};
use specsheet_data::mod_names::{game_paths, learn_word_swaps, recover_v1_names};
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
    let mut options = ExtractOptions {
        dry_run: flag("--dry-run", false).is_some(),
        skip_existing: flag("--skip-existing", false).is_some(),
        ..ExtractOptions::default()
    };
    if let Some(gb) = flag("--max-gb", true) {
        options.max_bytes = gb.parse::<u64>()?.saturating_mul(1 << 30);
    }
    let others = flag("--others", true);
    let no_cache = flag("--no-cache", false).is_some();
    let [package, dest] = <[String; 2]>::try_from(args).map_err(|_| {
        "usage: mod_extract [--dry-run] [--skip-existing] [--max-gb <n>] [--others <dir>] [--no-cache] <package> <destination>"
    })?;
    let package = PathBuf::from(package);

    let install = GameInstall::discover()?;
    let mut protected = vec![
        install.root().to_path_buf(),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."),
    ];
    if let Some(folder) = package.parent() {
        protected.push(folder.to_path_buf());
    }
    if let Some(home) = std::env::var_os("USERPROFILE") {
        protected.push(
            Path::new(&home)
                .join("Documents")
                .join("Euro Truck Simulator 2"),
        );
    }

    let done = if let Ok(zip) = ZipArchive::open(&package) {
        extract(&Archive::Zip(&zip), Path::new(&dest), &protected, options)?
    } else {
        let archive = HashFsV1Archive::open(&package)?;
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
        let names = recover_v1_names(&archive, candidates, &swaps);
        println!(
            "{}: {} entries, {} named, {} unnamed",
            package.display(),
            archive.len(),
            names.names.len(),
            names.unnamed.len()
        );
        extract(
            &Archive::V1 {
                archive: &archive,
                names: &names,
            },
            Path::new(&dest),
            &protected,
            options,
        )?
    };
    let verb = if options.dry_run {
        "would write"
    } else {
        "wrote"
    };
    println!(
        "{verb} {} files ({:.1} MB), {} under _unnamed/, {} renamed, {} skipped, {} unreadable",
        done.written,
        done.bytes as f64 / 1e6,
        done.unnamed,
        done.renamed.len(),
        done.skipped,
        done.unreadable.len()
    );
    for (from, to) in done.renamed.iter().take(10) {
        println!("    renamed {from} -> {to}");
    }
    Ok(())
}
