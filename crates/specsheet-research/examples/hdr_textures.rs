//! Research tool: lists every R9G9B9E5_SHAREDEXP (DXGI 67) texture and
//! checks that it unpacks and decodes to plausible linear HDR. For each
//! texture it prints the size, faces and mips, and the mean and peak colour
//! of the base mip, decoded as in the DXGI format definition: three 9-bit
//! mantissas (bits 0–8, 9–17, 18–26) scaled by 2^(exponent − 24), exponent
//! in bits 27–31.
//!
//! Usage: cargo run --release -p specsheet-research --example hdr_textures -- <archive.scs>...

use std::collections::VecDeque;

use specsheet_archive::{Archive, Content, EntryKind};

const RGB9E5: u32 = 67;

fn main() {
    let (mut found, mut failed) = (0, 0);
    for path in std::env::args().skip(1) {
        let archive = match Archive::open(&path) {
            Ok(archive) => archive,
            Err(e) => {
                println!("{path}: ERROR {e}");
                continue;
            }
        };
        for file in files(&archive) {
            let Some(Content::Texture { image, .. }) = archive.entry(&file).map(|e| &e.content)
            else {
                continue;
            };
            if image.dxgi_format != RGB9E5 {
                continue;
            }
            found += 1;
            let summary = archive
                .read_texture(&file)
                .map_err(|e| e.to_string())
                .and_then(|packed| packed.unpack())
                .map(|surfaces| {
                    let base = surfaces.iter().find(|s| s.mip == 0 && s.face == 0);
                    base.map(|s| stats(&s.data)).unwrap_or_default()
                });
            match summary {
                Ok((mean, peak)) => println!(
                    "{file}: {}x{} faces {} mips {} mean {mean:.3?} peak {peak:.3?}",
                    image.width, image.height, image.face_count, image.mip_count
                ),
                Err(e) => {
                    failed += 1;
                    println!("{file}: UNPACK ERROR {e}");
                }
            }
        }
    }
    println!("DXGI {RGB9E5} textures: {found}, failed to unpack: {failed}");
}

/// Mean and per-channel peak of the decoded pixels.
fn stats(data: &[u8]) -> ([f32; 3], [f32; 3]) {
    let mut sum = [0.0f64; 3];
    let mut peak = [0.0f32; 3];
    let pixels = data.len() / 4;
    for pixel in data.chunks_exact(4) {
        let rgb = decode(u32::from_le_bytes([pixel[0], pixel[1], pixel[2], pixel[3]]));
        for c in 0..3 {
            sum[c] += f64::from(rgb[c]);
            peak[c] = peak[c].max(rgb[c]);
        }
    }
    let mean = sum.map(|s| (s / pixels.max(1) as f64) as f32);
    (mean, peak)
}

fn decode(word: u32) -> [f32; 3] {
    let scale = 2f32.powi((word >> 27) as i32 - 24);
    [0, 9, 18].map(|shift| ((word >> shift) & 0x1ff) as f32 * scale)
}

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
