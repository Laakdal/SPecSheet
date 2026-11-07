//! Research tool over a whole installation: every archive, layered in the
//! game's lookup order (`GameInstall::mount`).
//!
//! Usage (ETS2_DIR must point at the game directory):
//!   cargo run --release -p specsheet-research --example scs -- ls <dir>
//!   cargo run --release -p specsheet-research --example scs -- cat <file>
//!   cargo run --release -p specsheet-research --example scs -- find <dir> <substring>
//!   cargo run --release -p specsheet-research --example scs -- tex <file.tobj>   (image and sample records)
//!   cargo run --release -p specsheet-research --example scs -- version   (the installed game version)

use std::io::Write;

use specsheet_archive::Vfs;
use specsheet_data::GameInstall;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // The game's lookup order (F2/F14): DLCs by their manifest's
    // relative_priority, the core pool on top (`GameInstall::mount`).
    let (vfs, failures) = GameInstall::discover()?.mount()?;
    for (path, error) in &failures {
        eprintln!("not mounted: {}: {error}", path.display());
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
        ["version"] => println!("{}", GameInstall::discover()?.version()?),
        ["find", root, needle] => find(&vfs, root, needle)?,
        ["tex", path] => {
            let texture = vfs.read_texture(path)?;
            println!("{:?}", texture.image);
            println!("{:?}", texture.sample);
            println!("{} decompressed bytes", texture.surfaces.len());
            // Per face, the colour of the smallest mip: for BC formats the
            // mean of the block's two RGB565 endpoints (sRGB-encoded).
            let surfaces = texture.unpack()?;
            for face in 0..texture.image.face_count {
                let Some(last) = surfaces.iter().rev().find(|s| s.face == face) else {
                    continue;
                };
                let rgb = match texture.image.dxgi_format {
                    70..=72 => endpoint_mean(&last.data[..4]),
                    73..=78 => endpoint_mean(&last.data[8..12]),
                    _ => continue,
                };
                println!("face {face}: smallest-mip colour {rgb:.3?}");
            }
        }
        _ => eprintln!("usage: scs ls <dir> | cat <file> | find <dir> <substring> | tex <file>"),
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

/// Mean of the two RGB565 endpoint colours of a BC colour block.
fn endpoint_mean(block: &[u8]) -> [f32; 3] {
    let colour = |lo: u8, hi: u8| {
        let v = u16::from_le_bytes([lo, hi]);
        [
            f32::from(v >> 11) / 31.0,
            f32::from((v >> 5) & 0x3f) / 63.0,
            f32::from(v & 0x1f) / 31.0,
        ]
    };
    let (a, b) = (colour(block[0], block[1]), colour(block[2], block[3]));
    [0, 1, 2].map(|i| (a[i] + b[i]) / 2.0)
}
