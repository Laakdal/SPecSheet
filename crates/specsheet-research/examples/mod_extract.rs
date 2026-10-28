//! Extracts a mod package (HashFS v1 or ZIP) to a folder (D19,
//! `specsheet_data::extract`). File names of a v1 package are recovered
//! first (`mod_names`), with every path of the mounted game as candidates
//! and, with `--others <dir>`, the paths that the other packages in `<dir>`
//! mention. The destination must be absolute and may not overlap the game
//! install, the ETS2 home folder, the folder holding the package, or this
//! repository. Nothing is overwritten.
//!
//! Usage (ETS2_DIR set, or the install is discovered):
//!   cargo run --release -p specsheet-research --example mod_extract -- [--dry-run] [--skip-existing] [--others <dir>] <package> <destination>

use std::path::{Path, PathBuf};

use specsheet_archive::{HashFsV1Archive, ZipArchive};
use specsheet_data::extract::{extract, Archive, ExtractOptions};
use specsheet_data::mod_names::{game_paths, recover_v1_names};
use specsheet_data::GameInstall;
use specsheet_formats::paths::referenced_paths;

/// v1 entries larger than this are not scanned (as in `mod_names`).
const MAX_SCAN: u32 = 1 << 20;

/// Paths the packages in `dir` hold or mention.
fn other_packages(dir: &str) -> std::io::Result<Vec<String>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if let Ok(zip) = ZipArchive::open(&path) {
            for file in zip.files() {
                out.push(file.to_owned());
                if let Ok(bytes) = zip.read(file) {
                    out.extend(referenced_paths(file, &bytes));
                }
            }
        } else if let Ok(v1) = HashFsV1Archive::open(&path) {
            for entry in v1.entries() {
                if !entry.is_directory() && entry.size <= MAX_SCAN {
                    if let Ok(bytes) = v1.read_entry(entry) {
                        out.extend(referenced_paths("", &bytes));
                    }
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
    let options = ExtractOptions {
        dry_run: flag("--dry-run", false).is_some(),
        skip_existing: flag("--skip-existing", false).is_some(),
    };
    let others = flag("--others", true);
    let [package, dest] = <[String; 2]>::try_from(args).map_err(|_| {
        "usage: mod_extract [--dry-run] [--skip-existing] [--others <dir>] <package> <destination>"
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
        if let Some(dir) = &others {
            candidates.extend(other_packages(dir)?);
        }
        let names = recover_v1_names(&archive, candidates);
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
