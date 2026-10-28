//! Game UI images as egui textures: `<img src=…>` materials loaded from the
//! install on first use. Distance-field materials (category icons, font
//! pages) become white images whose alpha is the shape's coverage, so a
//! tint colours them as the game's `color=` does.

use std::collections::HashMap;

use bevy_egui::egui;
use specsheet_data::render::{load_texture, TextureData, TextureFormat};
use specsheet_gameui::load_ui_material;
use specsheet_session::GameSession;

pub struct UiTexture {
    pub id: egui::TextureId,
    /// Texel size.
    pub size: [f32; 2],
    // Keeps the GPU texture alive.
    _handle: egui::TextureHandle,
}

/// Every UI image requested so far, by `src`; `None` when unusable.
#[derive(Default)]
pub struct UiTextures {
    loaded: HashMap<String, Option<UiTexture>>,
    /// Problems, newest last.
    pub warnings: Vec<String>,
}

impl UiTextures {
    pub fn get(
        &mut self,
        ctx: &egui::Context,
        game: &GameSession,
        src: &str,
    ) -> Option<&UiTexture> {
        if !self.loaded.contains_key(src) {
            let texture = match load(ctx, game, src) {
                Ok(texture) => Some(texture),
                Err(error) => {
                    bevy::log::warn!("UI image {src}: {error}");
                    self.warnings.push(format!("{src}: {error}"));
                    None
                }
            };
            self.loaded.insert(src.to_owned(), texture);
        }
        self.loaded.get(src).and_then(Option::as_ref)
    }
}

fn load(ctx: &egui::Context, game: &GameSession, src: &str) -> Result<UiTexture, String> {
    let material = load_ui_material(game.fs(), src).map_err(|e| e.to_string())?;
    let path = material.texture.ok_or("material names no texture")?;
    let data = load_texture(game.fs(), &path).map_err(|e| e.to_string())?;
    let field = match (material.distance_field, material.effect.contains("msdf")) {
        (false, _) => Field::None,
        (true, true) => Field::MultiChannel,
        (true, false) => Field::SingleChannel,
    };
    let image = color_image(&data, field)?;
    let size = [image.size[0] as f32, image.size[1] as f32];
    let handle = ctx.load_texture(src, image, egui::TextureOptions::LINEAR);
    Ok(UiTexture {
        id: handle.id(),
        size,
        _handle: handle,
    })
}

/// How a UI texture encodes its shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    /// A plain image.
    None,
    /// One distance channel (`ui.sdf.rfx`); the others may be empty.
    SingleChannel,
    /// Distance is the median of RGB (`ui.font.msdf.rfx`).
    MultiChannel,
}

/// Largest texture side accepted, as for 3D textures (`assets.rs`).
const MAX_SIDE: u32 = 16384;

/// Edge softness of distance fields, in distance units around 0.5.
const DISTANCE_SOFTNESS: f32 = 0.08;

/// The first mip as an egui image. Only uncompressed formats are handled;
/// UI textures in 1.60 are B8G8R8A8/X8 (docs/formats/ui.md).
pub fn color_image(data: &TextureData, field: Field) -> Result<egui::ColorImage, String> {
    let mip = data.mips.first().ok_or("texture has no surfaces")?;
    if mip.width == 0 || mip.height == 0 || mip.width > MAX_SIDE || mip.height > MAX_SIDE {
        return Err(format!("unsupported size {}x{}", mip.width, mip.height));
    }
    let (w, h) = (mip.width as usize, mip.height as usize);
    let pixels = w.checked_mul(h).ok_or("texture too large")?;
    let rgba: Vec<u8> = match data.format {
        TextureFormat::Rgba8 if mip.data.len() == pixels * 4 => mip.data.clone(),
        TextureFormat::R8 if mip.data.len() == pixels => {
            mip.data.iter().flat_map(|&v| [v, v, v, 255]).collect()
        }
        other => return Err(format!("unsupported UI texture format {other:?}")),
    };
    if field == Field::None {
        return Ok(egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba));
    }
    let coverage = |d: f32| {
        let t = ((d - 0.5 + DISTANCE_SOFTNESS) / (2.0 * DISTANCE_SOFTNESS)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    let shape: Vec<u8> = rgba
        .chunks_exact(4)
        .flat_map(|p| {
            let [r, g, b] = [p[0], p[1], p[2]].map(|c| f32::from(c) / 255.0);
            let distance = match field {
                Field::MultiChannel => r.max(g).min(r.min(g).max(b)),
                _ => r.max(g).max(b),
            };
            [255, 255, 255, (coverage(distance) * 255.0).round() as u8]
        })
        .collect();
    Ok(egui::ColorImage::from_rgba_unmultiplied([w, h], &shape))
}

#[cfg(test)]
mod tests {
    use super::*;
    use specsheet_data::render::MipLevel;

    fn rgba(pixels: &[[u8; 4]], width: u32) -> TextureData {
        TextureData {
            format: TextureFormat::Rgba8,
            srgb: true,
            mips: vec![MipLevel {
                width,
                height: pixels.len() as u32 / width,
                data: pixels.iter().flatten().copied().collect(),
            }],
            repeat_u: false,
            repeat_v: false,
        }
    }

    #[test]
    fn distance_fields_become_white_coverage() {
        let data = rgba(
            &[[255, 255, 255, 255], [0, 0, 0, 255], [128, 128, 128, 255]],
            3,
        );
        let image = color_image(&data, Field::MultiChannel).expect("image");
        let alpha: Vec<u8> = image.pixels.iter().map(|p| p.a()).collect();
        assert_eq!(alpha[0], 255, "inside");
        assert_eq!(alpha[1], 0, "outside");
        assert!((100..160).contains(&alpha[2]), "edge {}", alpha[2]);
        // A single-channel field in red only: the median would hide it.
        let red = rgba(&[[255, 0, 0, 255]], 1);
        let image = color_image(&red, Field::SingleChannel).expect("image");
        assert_eq!(image.pixels[0].a(), 255);
    }

    #[test]
    fn rejects_empty_and_oversized_textures() {
        let mut empty = rgba(&[], 1);
        empty.mips[0].width = 0;
        empty.mips[0].height = 0;
        assert!(color_image(&empty, Field::None).is_err());
        let mut huge = rgba(&[[0; 4]], 1);
        huge.mips[0].width = MAX_SIDE + 1;
        assert!(color_image(&huge, Field::None).is_err());
    }

    #[test]
    fn rejects_mismatched_or_compressed_data() {
        let mut data = rgba(&[[1, 2, 3, 4]], 1);
        data.mips[0].data.pop();
        assert!(color_image(&data, Field::None).is_err());
        data.format = TextureFormat::Bc1;
        assert!(color_image(&data, Field::None).is_err());
    }
}
