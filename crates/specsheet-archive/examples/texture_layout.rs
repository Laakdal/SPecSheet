//! Research tool: tests the packed-texture layout against the bytes. For a
//! sample of textures per (format, face count), it predicts the decompressed
//! surface size from the image record under four padding hypotheses and
//! counts which one matches exactly. It also tallies the cube flag against
//! the face count, and the sample record's address modes per top-level
//! directory.
//!
//! Usage: cargo run --release -p specsheet-archive --example texture_layout -- <archive.scs>...

use std::collections::{BTreeMap, VecDeque};

use specsheet_archive::{pixel_layout, Archive, Content, EntryKind, ImageMeta, PixelLayout};

/// Textures decompressed per (DXGI format, face count) bucket.
const PER_BUCKET: usize = 25;

const HYPOTHESES: [&str; 4] = [
    "tight",
    "image alignment only",
    "pitch alignment only",
    "pitch + image alignment",
];

fn main() {
    let mut buckets: BTreeMap<(u32, u32), [usize; 5]> = BTreeMap::new();
    let mut addressing: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut examples = Vec::new();
    let mut rare: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut cube_flags: BTreeMap<(bool, u32), usize> = BTreeMap::new();
    for path in std::env::args().skip(1) {
        let archive = match Archive::open(&path) {
            Ok(archive) => archive,
            Err(e) => {
                println!("{path}: ERROR {e}");
                continue;
            }
        };
        for file in files(&archive) {
            let Some(Content::Texture { image, sample, .. }) =
                archive.entry(&file).map(|e| &e.content)
            else {
                continue;
            };
            *cube_flags
                .entry((image.is_cube, image.face_count))
                .or_default() += 1;
            let top = file.split('/').next().unwrap_or_default().to_owned();
            let modes = format!(
                "mag_linear={} min_linear={} u={:?} v={:?}",
                sample.mag_linear, sample.min_linear, sample.address_u, sample.address_v
            );
            *addressing.entry((top, modes.clone())).or_default() += 1;
            let unusual = !sample.mag_linear
                || format!("{:?}", sample.address_u).contains("Border")
                || pixel_layout(image.dxgi_format).is_none();
            if unusual {
                let paths = rare
                    .entry(format!("{modes} dxgi={}", image.dxgi_format))
                    .or_default();
                if paths.len() < 4 {
                    paths.push(file.clone());
                }
            }

            let bucket = buckets
                .entry((image.dxgi_format, image.face_count))
                .or_default();
            if bucket[4] >= PER_BUCKET {
                continue;
            }
            let Ok(texture) = archive.read_texture(&file) else {
                continue;
            };
            bucket[4] += 1;
            let actual = texture.surfaces.len();
            for (h, slot) in bucket.iter_mut().take(4).enumerate() {
                if predicted_size(image, h) == Some(actual) {
                    *slot += 1;
                }
            }
            if examples.len() < 12 && image.mip_count > 1 {
                examples.push(format!(
                    "  {file}: {}x{} mips {} dxgi {} faces {} pitch {} image {} -> {actual} bytes",
                    image.width,
                    image.height,
                    image.mip_count,
                    image.dxgi_format,
                    image.face_count,
                    image.pitch_alignment,
                    image.image_alignment
                ));
            }
        }
    }
    println!("Exact size matches per hypothesis ({HYPOTHESES:?}), out of n:");
    for ((format, faces), counts) in &buckets {
        println!(
            "  dxgi {format:3} faces {faces}: {:?} of n={}",
            &counts[..4],
            counts[4]
        );
    }
    println!("Examples:");
    for line in examples {
        println!("{line}");
    }
    println!("Textures by (cube flag, face count): {cube_flags:?}");
    println!("Unusual sampler states or formats (up to 4 paths each):");
    for (state, paths) in rare {
        println!("  {state}: {paths:?}");
    }
    println!("Sample record by top-level directory:");
    for ((top, modes), n) in addressing {
        println!("  {top:12} {modes}: {n}");
    }
}

/// Decompressed size predicted under hypothesis `h` (index into `HYPOTHESES`).
/// `None` for formats `pixel_layout` does not know.
fn predicted_size(image: &ImageMeta, h: usize) -> Option<usize> {
    let layout = pixel_layout(image.dxgi_format)?;
    let pitch = if h >= 2 { image.pitch_alignment } else { 1 } as usize;
    let block = if h == 1 || h == 3 {
        image.image_alignment
    } else {
        1
    } as usize;
    let mut size = 0usize;
    for _ in 0..image.face_count {
        let (mut width, mut height) = (image.width, image.height);
        for _ in 0..image.mip_count {
            let (row_bytes, rows) = match layout {
                PixelLayout::Block { bytes } => {
                    (width.div_ceil(4).max(1) * bytes, height.div_ceil(4).max(1))
                }
                PixelLayout::Linear { bytes_per_pixel } => (width * bytes_per_pixel, height),
            };
            // Padding goes before each mip and each row, never after the
            // last one: the final row of the file ends the data.
            size = size.next_multiple_of(block);
            for _ in 0..rows {
                size = size.next_multiple_of(pitch) + row_bytes as usize;
            }
            width = (width / 2).max(1);
            height = (height / 2).max(1);
        }
    }
    Some(size)
}

/// Every file path reachable from the archive's root listing.
fn files(archive: &Archive) -> Vec<String> {
    let mut out = Vec::new();
    let mut queue = VecDeque::from([String::new()]);
    while let Some(dir) = queue.pop_front() {
        let Ok(listing) = archive.list_dir(&dir) else {
            continue;
        };
        let join = |name: &String| {
            if dir.is_empty() {
                name.clone()
            } else {
                format!("{dir}/{name}")
            }
        };
        for name in &listing.directories {
            let path = join(name);
            if archive.entry(&path).map(|e| e.kind) == Some(EntryKind::Directory) {
                queue.push_back(path);
            }
        }
        out.extend(listing.files.iter().map(join));
    }
    out
}
