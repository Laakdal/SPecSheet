//! Research tool over a whole installation (all archives layered).
//!
//! Usage (ETS2_DIR must point at the game directory):
//!   cargo run --release --example scs -- ls <dir>
//!   cargo run --release --example scs -- cat <file>
//!   cargo run --release --example scs -- find <dir> <substring>

use std::io::Write;

use specsheet_archive::{Archive, Vfs};

const BASE_ORDER: &[&str] = &[
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::var("ETS2_DIR")?;
    let mut vfs = Vfs::new();
    for name in BASE_ORDER {
        vfs.mount(Archive::open(format!("{dir}/{name}"))?);
    }
    let mut dlcs: Vec<_> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("dlc_") && n.ends_with(".scs"))
        .collect();
    dlcs.sort();
    for name in dlcs {
        vfs.mount(Archive::open(format!("{dir}/{name}"))?);
    }

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["ls", path] => {
            let listing = vfs.list_dir(path)?;
            for d in listing.directories {
                println!("{d}/");
            }
            for f in listing.files {
                println!("{f}");
            }
        }
        ["cat", path] => std::io::stdout().write_all(&vfs.read(path)?)?,
        ["find", root, needle] => find(&vfs, root, needle)?,
        _ => eprintln!("usage: scs ls <dir> | cat <file> | find <dir> <substring>"),
    }
    Ok(())
}

fn find(vfs: &Vfs, root: &str, needle: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut stack = vec![root.trim_matches('/').to_owned()];
    while let Some(dir) = stack.pop() {
        let listing = vfs.list_dir(&dir)?;
        let join = |n: &str| {
            if dir.is_empty() {
                n.to_owned()
            } else {
                format!("{dir}/{n}")
            }
        };
        for f in &listing.files {
            let p = join(f);
            if p.contains(needle) {
                println!("{p}");
            }
        }
        stack.extend(listing.directories.iter().map(|d| join(d)));
    }
    Ok(())
}
