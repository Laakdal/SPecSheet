//! Research tool: the header of standalone (loose) `.tobj` files in mod
//! packages, and the `.dds` headers they name. Where the game holds a
//! packed texture at the same `.tobj` path, its decoded sample and image
//! records are set beside the loose header bytes, so each byte's meaning
//! can be read off our own data (`research/evidence/tobj-loose.txt`).
//! Packages are only read; output is counts only.
//!
//! Usage (ETS2_DIR set, or the install is discovered):
//!   cargo run --release -p specsheet-research --example tobj_survey -- "<home>/mod"

use std::collections::BTreeMap;

use specsheet_archive::{Content, HashFsV1Archive, ZipArchive};
use specsheet_data::mod_names::{recover_v1_names, WordSwaps};
use specsheet_data::GameInstall;
use specsheet_formats::tobj::texture_paths;

/// Header bytes whose values vary between files (0x00..0x28; the rest are
/// constant in every package checked first, see the evidence file).
const VARYING: [usize; 7] = [0x18, 0x1a, 0x1b, 0x1c, 0x1e, 0x1f, 0x20];

type Table = BTreeMap<String, BTreeMap<String, usize>>;

fn tally(table: &mut Table, row: String, column: String) {
    *table.entry(row).or_default().entry(column).or_default() += 1;
}

/// The first texture a `.tobj` names, without a leading slash.
fn first_texture(tobj: &[u8]) -> Option<String> {
    let paths = texture_paths(tobj)?;
    paths.first().map(|p| p.trim_start_matches('/').to_owned())
}

/// The DDS format of a header: `DX10 <dxgi>` or the legacy FourCC.
fn dds_kind(dds: &[u8]) -> String {
    let Some(four) = dds.get(84..88) else {
        return "short".to_owned();
    };
    if four == b"DX10" {
        let dxgi = dds
            .get(128..132)
            .map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        format!("DX10 {dxgi}")
    } else if four == [0, 0, 0, 0] {
        let bits = dds
            .get(88..92)
            .map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        format!("uncompressed {bits}-bit")
    } else {
        format!("FourCC {}", String::from_utf8_lossy(four))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::args()
        .nth(1)
        .ok_or("usage: tobj_survey <mod folder>")?;
    let (vfs, _) = GameInstall::discover()?.mount()?;
    let mut headers: Table = Table::new();
    let mut versus_game: Table = Table::new();
    let mut formats: BTreeMap<String, usize> = BTreeMap::new();
    let (mut loose, mut with_game, mut constant_breaks) = (0usize, 0usize, 0usize);
    let mut constant: Option<Vec<u8>> = None;
    // Per header byte (0x04..0x28): its values over all loose files.
    let mut per_byte: BTreeMap<usize, BTreeMap<u8, usize>> = BTreeMap::new();
    // For loose files whose DDS carries no colour space (legacy FourCC or
    // uncompressed) and that the game packs too: header byte value against
    // the game's sRGB flag.
    let mut versus_srgb: Table = Table::new();

    let mut files: Vec<_> = std::fs::read_dir(&dir)?
        .flatten()
        .map(|e| e.path())
        .collect();
    files.sort();
    for file in files {
        // (path, tobj bytes, dds bytes) of every loose texture in the package.
        let mut found: Vec<(String, Vec<u8>, Option<Vec<u8>>)> = Vec::new();
        if let Ok(zip) = ZipArchive::open(&file) {
            for path in zip.files().filter(|p| p.ends_with(".tobj")) {
                let Ok(tobj) = zip.read(path) else { continue };
                let dds = first_texture(&tobj).and_then(|d| zip.read(&d).ok());
                found.push((path.to_owned(), tobj, dds));
            }
        } else if let Ok(v1) = HashFsV1Archive::open(&file) {
            let names = recover_v1_names(&v1, Vec::new(), &WordSwaps::default());
            for path in names.names.values().filter(|p| p.ends_with(".tobj")) {
                let Ok(tobj) = v1.read(path) else { continue };
                let dds = first_texture(&tobj).and_then(|d| v1.read(&d).ok());
                found.push((path.clone(), tobj, dds));
            }
        }
        for (path, tobj, dds) in found {
            if tobj.len() < 0x28 || !tobj.starts_with(&[0x01, 0x0a, 0xb1, 0x70]) {
                continue;
            }
            loose += 1;
            for (at, value) in tobj.iter().enumerate().take(0x28).skip(4) {
                *per_byte.entry(at).or_default().entry(*value).or_default() += 1;
            }
            let mut fixed = tobj[..0x28].to_vec();
            for at in VARYING {
                fixed[at] = 0;
            }
            match &constant {
                None => constant = Some(fixed),
                Some(c) if *c != fixed => constant_breaks += 1,
                Some(_) => {}
            }
            let bytes: Vec<String> = VARYING
                .iter()
                .map(|&at| format!("{at:#04x}={}", tobj[at]))
                .collect();
            let kind = dds.as_deref().map_or("missing".to_owned(), dds_kind);
            *formats.entry(kind.clone()).or_default() += 1;
            tally(&mut headers, bytes.join(" "), kind.clone());
            let game = vfs
                .locate(&path)
                .and_then(|d| d.as_v2())
                .and_then(|a| a.entry(&path))
                .and_then(|e| match &e.content {
                    Content::Texture { image, sample, .. } => Some((*image, *sample)),
                    _ => None,
                });
            if let Some((image, _)) = game.filter(|_| !kind.starts_with("DX10")) {
                let srgb = matches!(image.dxgi_format, 29 | 72 | 75 | 78 | 91 | 93 | 99);
                for (at, value) in tobj.iter().enumerate().take(0x28).skip(4) {
                    tally(
                        &mut versus_srgb,
                        format!("{at:#04x}={value}"),
                        format!("game srgb {}", u8::from(srgb)),
                    );
                }
            }
            if let Some((image, sample)) = game {
                with_game += 1;
                for &at in &VARYING {
                    let value = format!(
                        "game: mag {} min {} u {:?} v {:?} cube {} mips {}",
                        u8::from(sample.mag_linear),
                        u8::from(sample.min_linear),
                        sample.address_u,
                        sample.address_v,
                        u8::from(image.is_cube),
                        u8::from(image.mip_count > 1)
                    );
                    tally(&mut versus_game, format!("{at:#04x}={}", tobj[at]), value);
                }
            }
        }
    }
    println!("loose .tobj: {loose}; header bytes outside {VARYING:02x?} differ from the first file in {constant_breaks}");
    println!("dds formats: {formats:?}");
    println!("header bytes with more than one value:");
    for (at, values) in &per_byte {
        if values.len() > 1 {
            println!("  {at:#04x}: {values:?}");
        }
    }
    println!("legacy or uncompressed DDS with a packed game texture, header byte -> game sRGB:");
    for (row, columns) in &versus_srgb {
        // Only bytes that vary say anything.
        let varies = row
            .get(2..4)
            .and_then(|hex| usize::from_str_radix(hex, 16).ok())
            .and_then(|at| per_byte.get(&at))
            .is_some_and(|values| values.len() > 1);
        if varies {
            println!("  {row}: {columns:?}");
        }
    }
    println!("varying header bytes -> dds format:");
    for (row, columns) in &headers {
        println!("  {row}: {columns:?}");
    }
    println!("with a packed game texture at the same path: {with_game}");
    for (row, columns) in &versus_game {
        println!("  {row}: {columns:?}");
    }
    Ok(())
}
