//! Renderer-independent mesh, material and texture data built from original
//! game files (see `docs/RENDERING.md`).
//!
//! Conventions applied here, once, so renderers need no SCS knowledge:
//! * Triangle winding is flipped to counter-clockwise front faces (game data
//!   is clockwise: 99.3 % of sampled triangles, `research/evidence/geometry-survey.txt`).
//! * Only parts visible in the requested variant are emitted.
//! * Texture surfaces are unpacked; BGRA is swizzled to RGBA.

use scs_archive::{pixel_layout, PackedTexture, PixelLayout};
use scs_formats::mat::{MatValue, Material};
use scs_formats::sii::resolve_relative;

use crate::error::{GameDataError, Result};
use crate::fs::GameFs;
use crate::model::Model;

/// How a material is drawn, derived from its effect name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendMode {
    Opaque,
    /// Alpha-tested cut-out (`.a` effects).
    AlphaTest,
    /// Alpha-blended (glass, decals, overlays, water).
    Blend,
    /// Additively blended (`add` not followed by `env`, e.g. light glows).
    Additive,
    /// Not drawn in a preview (shadow-only and helper effects).
    Hidden,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialInfo {
    pub path: String,
    pub effect: String,
    pub blend: BlendMode,
    pub diffuse: [f32; 3],
    /// `.tobj` path of the base colour texture.
    pub base_texture: Option<String>,
    /// Painted with the truck's paint-job colour (`truckpaint` effects).
    pub paintable: bool,
    /// Self-lit (lamps, luminance effects).
    pub emissive: bool,
    /// Decals and overlays sit on other geometry and need depth biasing.
    pub decal: bool,
}

impl MaterialInfo {
    fn fallback(path: &str) -> Self {
        Self {
            path: path.to_owned(),
            effect: String::new(),
            blend: BlendMode::Opaque,
            diffuse: [0.6, 0.6, 0.6],
            base_texture: None,
            paintable: false,
            emissive: false,
            decal: false,
        }
    }

    fn from_material(path: &str, material: &Material) -> Self {
        let tokens: Vec<&str> = material.effect.split('.').collect();
        let has = |t: &str| tokens.contains(&t);
        let blend = if ["shadowonly", "fakeshadow", "decalshadow", "mlaaweight"]
            .iter()
            .any(|t| has(t))
        {
            BlendMode::Hidden
        } else if tokens.windows(2).any(|w| w[0] == "add" && w[1] != "env") {
            BlendMode::Additive
        } else if has("glass") || has("decal") || has("over") || has("water") {
            BlendMode::Blend
        } else if has("a") {
            BlendMode::AlphaTest
        } else {
            BlendMode::Opaque
        };
        let diffuse = material
            .attribute("diffuse")
            .and_then(MatValue::as_floats)
            .and_then(|v| <[f32; 3]>::try_from(v.get(..3)?).ok())
            .unwrap_or([1.0; 3]);
        let base_texture = material
            .texture("texture_base")
            .and_then(|t| t.source.as_deref())
            .map(|source| resolve_relative(path, source));
        Self {
            path: path.to_owned(),
            effect: material.effect.clone(),
            blend,
            diffuse,
            base_texture,
            paintable: has("truckpaint"),
            emissive: has("lamp") || has("lum") || has("light"),
            decal: has("decal") || has("over"),
        }
    }
}

/// One drawable piece: flat vertex attribute arrays plus CCW indices.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshPiece {
    pub material: usize,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// First UV set, if present.
    pub uvs: Option<Vec<[f32; 2]>>,
    pub indices: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeshData {
    pub path: String,
    pub materials: Vec<MaterialInfo>,
    pub pieces: Vec<MeshPiece>,
    pub aabb_min: [f32; 3],
    pub aabb_max: [f32; 3],
}

/// Builds the drawable mesh of `model` for one look and variant, limited
/// to the named `parts` (empty: every part). Materials that cannot be read
/// fall back to a neutral grey and are reported.
pub fn build_mesh(
    fs: &GameFs,
    model: &Model,
    look: Option<&str>,
    variant: Option<&str>,
    parts: &[String],
) -> (MeshData, Vec<String>) {
    let mut warnings = Vec::new();
    let pmd_look = look
        .and_then(|name| model.pmd.look(name))
        .or_else(|| model.pmd.looks.first());
    let materials: Vec<MaterialInfo> = pmd_look
        .map(|l| l.materials.as_slice())
        .unwrap_or_default()
        .iter()
        .map(|path| match load_material(fs, path) {
            Ok(material) => material,
            Err(error) => {
                warnings.push(format!("{path}: {error}"));
                MaterialInfo::fallback(path)
            }
        })
        .collect();

    let variant = model.variant_index(variant);
    let mut pieces = Vec::new();
    for (part_index, part) in model.pmg.parts.iter().enumerate() {
        let selected = parts.is_empty() || parts.contains(&part.name);
        if !selected || !model.pmd.is_part_visible(variant, part_index) {
            continue;
        }
        for piece in &model.pmg.pieces[part.pieces.clone()] {
            let material = piece.material as usize;
            if materials
                .get(material)
                .is_some_and(|m| m.blend == BlendMode::Hidden)
            {
                continue;
            }
            let indices = piece
                .indices
                .chunks_exact(3)
                .flat_map(|t| [u32::from(t[0]), u32::from(t[2]), u32::from(t[1])])
                .collect();
            pieces.push(MeshPiece {
                material,
                positions: piece.positions.clone(),
                normals: piece.normals.clone(),
                uvs: piece.uv_sets.first().cloned(),
                indices,
            });
        }
    }
    let mesh = MeshData {
        path: model.path.clone(),
        materials,
        pieces,
        aabb_min: model.pmg.aabb.min,
        aabb_max: model.pmg.aabb.max,
    };
    (mesh, warnings)
}

fn load_material(fs: &GameFs, path: &str) -> Result<MaterialInfo> {
    let material = Material::parse(&fs.read(path)?).map_err(|source| GameDataError::Format {
        path: path.to_owned(),
        source,
    })?;
    Ok(MaterialInfo::from_material(path, &material))
}

/// GPU upload format of a texture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureFormat {
    Bc1,
    Bc2,
    Bc3,
    Bc4,
    Bc5,
    Rgba8,
    R8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MipLevel {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

/// A 2D texture ready for upload: tightly packed mip chain of face 0.
#[derive(Clone, Debug, PartialEq)]
pub struct TextureData {
    pub format: TextureFormat,
    pub srgb: bool,
    pub mips: Vec<MipLevel>,
    pub repeat_u: bool,
    pub repeat_v: bool,
}

/// Loads `.tobj` texture `path` and converts it for upload.
pub fn load_texture(fs: &GameFs, path: &str) -> Result<TextureData> {
    let packed = fs.vfs().read_texture(path.trim_start_matches('/'))?;
    convert_texture(path, &packed)
}

fn convert_texture(path: &str, packed: &PackedTexture) -> Result<TextureData> {
    let format_error = |message: String| GameDataError::Definition {
        path: path.to_owned(),
        message,
    };
    let dxgi = packed.image.dxgi_format;
    let (format, srgb, swizzle_bgra) = match dxgi {
        70 | 71 => (TextureFormat::Bc1, false, false),
        72 => (TextureFormat::Bc1, true, false),
        73 | 74 => (TextureFormat::Bc2, false, false),
        75 => (TextureFormat::Bc2, true, false),
        76 | 77 => (TextureFormat::Bc3, false, false),
        78 => (TextureFormat::Bc3, true, false),
        79..=81 => (TextureFormat::Bc4, false, false),
        82..=84 => (TextureFormat::Bc5, false, false),
        27 | 28 => (TextureFormat::Rgba8, false, false),
        29 => (TextureFormat::Rgba8, true, false),
        87 | 88 => (TextureFormat::Rgba8, false, true),
        90..=93 => (TextureFormat::Rgba8, dxgi == 91 || dxgi == 93, true),
        60..=64 => (TextureFormat::R8, false, false),
        other => return Err(format_error(format!("unsupported DXGI format {other}"))),
    };
    let x_channel_is_opaque = matches!(dxgi, 88 | 92 | 93);
    if pixel_layout(dxgi).is_none() {
        return Err(format_error(format!("no layout for DXGI format {dxgi}")));
    }
    let mips = packed
        .unpack()
        .map_err(format_error)?
        .into_iter()
        .filter(|s| s.face == 0)
        .map(|surface| {
            let mut data = surface.data;
            if swizzle_bgra {
                for pixel in data.chunks_exact_mut(4) {
                    pixel.swap(0, 2);
                    if x_channel_is_opaque {
                        pixel[3] = 255;
                    }
                }
            }
            MipLevel {
                width: surface.width,
                height: surface.height,
                data,
            }
        })
        .collect();
    let repeat = |mode: scs_archive::AddressMode| matches!(mode, scs_archive::AddressMode::Repeat);
    Ok(TextureData {
        format,
        srgb,
        mips,
        repeat_u: repeat(packed.sample.address_u),
        repeat_v: repeat(packed.sample.address_v),
    })
}

/// Bytes per block or pixel of an upload format, for consumers that
/// validate sizes.
pub fn texture_block_bytes(format: TextureFormat) -> (u32, PixelLayout) {
    match format {
        TextureFormat::Bc1 | TextureFormat::Bc4 => (8, PixelLayout::Block { bytes: 8 }),
        TextureFormat::Bc2 | TextureFormat::Bc3 | TextureFormat::Bc5 => {
            (16, PixelLayout::Block { bytes: 16 })
        }
        TextureFormat::Rgba8 => (4, PixelLayout::Linear { bytes_per_pixel: 4 }),
        TextureFormat::R8 => (1, PixelLayout::Linear { bytes_per_pixel: 1 }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn material(effect: &str) -> MaterialInfo {
        let source = format!(
            "effect : \"{effect}\" {{ diffuse : {{ 0.5 , 0.25 , 1 }} texture : \"texture_base\" {{ source : \"plain.tobj\" }} }}"
        );
        let mat = Material::parse(source.as_bytes()).expect("parse");
        MaterialInfo::from_material("/vehicle/truck/x/m.mat", &mat)
    }

    #[test]
    fn classifies_effects() {
        assert_eq!(material("eut2.shadowonly.rfx").blend, BlendMode::Hidden);
        assert_eq!(material("eut2.glass.rfx").blend, BlendMode::Blend);
        assert_eq!(
            material("eut2.dif.spec.add.env.a.decal.rfx").blend,
            BlendMode::Blend
        );
        assert_eq!(material("eut2.dif.spec.a.rfx").blend, BlendMode::AlphaTest);
        assert_eq!(
            material("eut2.dif.spec.add.env.rfx").blend,
            BlendMode::Opaque
        );
        assert!(material("eut2.truckpaint.tsnmapuv.rfx").paintable);
        assert!(material("eut2.lamp.add.env.rfx").emissive);
        assert_eq!(material("eut2.dif.lum.add.rfx").blend, BlendMode::Additive);
        assert_eq!(
            material("eut2.dif.spec.add.env.rfx").blend,
            BlendMode::Opaque
        );
        assert_eq!(material("eut2.water.rfx").blend, BlendMode::Blend);
    }

    #[test]
    fn resolves_relative_texture_paths_and_diffuse() {
        let m = material("eut2.dif.rfx");
        assert_eq!(
            m.base_texture.as_deref(),
            Some("/vehicle/truck/x/plain.tobj")
        );
        assert_eq!(m.diffuse, [0.5, 0.25, 1.0]);
    }
}
