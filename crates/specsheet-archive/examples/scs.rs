//! Research tool over a whole installation (all archives layered).
//!
//! Usage (ETS2_DIR must point at the game directory):
//!   cargo run --release --example scs -- ls <dir>
//!   cargo run --release --example scs -- cat <file>
//!   cargo run --release --example scs -- find <dir> <substring>
//!   cargo run --release --example scs -- tex <file.tobj>   (image and sample records)

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
