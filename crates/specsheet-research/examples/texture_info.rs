//! Research tool: a texture's upload format, colour space, sizes and, for
//! uncompressed formats, the mean of each channel of the largest mip.
//!
//! `cargo run --release -p specsheet-research --example texture_info -- <path.tobj>...`
//! With `TEXTURE_INFO_DUMP=<dir>`, also writes the largest mip's raw bytes
//! to `<dir>/<name>.top`, outside the repository.

use specsheet_data::render::{load_texture, TextureFormat};
use specsheet_data::{GameFs, GameInstall};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (vfs, _) = GameInstall::discover()?.mount()?;
    let fs = GameFs::new(vfs);
    for path in std::env::args().skip(1) {
        let texture = load_texture(&fs, &path)?;
        let top = texture.mips.first();
        println!(
            "{path}: {:?}, srgb {}, {} mips, top {:?}, repeat ({}, {})",
            texture.format,
            texture.srgb,
            texture.mips.len(),
            top.map(|m| (m.width, m.height)),
            texture.repeat_u,
            texture.repeat_v
        );
        if let (Ok(dir), Some(top)) = (std::env::var("TEXTURE_INFO_DUMP"), top) {
            let name = path.rsplit('/').next().unwrap_or("texture");
            std::fs::write(format!("{dir}/{name}.top"), &top.data)?;
        }
        let channels = match texture.format {
            TextureFormat::Rgba8 => 4,
            TextureFormat::R8 => 1,
            _ => continue,
        };
        if let Some(top) = top {
            let count = (top.data.len() / channels).max(1) as f64;
            let means: Vec<String> = (0..channels)
                .map(|c| {
                    let sum: f64 = top
                        .data
                        .iter()
                        .skip(c)
                        .step_by(channels)
                        .map(|&v| f64::from(v))
                        .sum();
                    format!("{:.1}", sum / count)
                })
                .collect();
            println!("  channel means (0..255): {}", means.join(", "));
        }
    }
    Ok(())
}
