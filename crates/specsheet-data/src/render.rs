//! Renderer-independent mesh, material and texture data built from original
//! game files (see `docs/RENDERING.md`).
//!
//! Conventions applied here, once, so renderers need no SCS knowledge:
//! * Triangle winding is flipped to counter-clockwise front faces (game data
//!   is clockwise: 99.3 % of sampled triangles, `research/evidence/geometry-survey.txt`).
//! * Only parts visible in the requested variant are emitted.
//! * Texture surfaces are unpacked; BGRA is swizzled to RGBA.

use specsheet_archive::{pixel_layout, PackedTexture, PixelLayout};
use specsheet_formats::mat::{MatValue, Material};
use specsheet_formats::sii::resolve_relative;

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
    /// Specular colour (`specular`), linear RGB.
    pub specular: Option<[f32; 3]>,
    /// Specular exponent (`shininess`), 0 to a few hundred.
    pub shininess: Option<f32>,
    /// Strength of the environment reflection per channel (`env_factor`).
    /// Chrome pairs a dark `diffuse` with a factor near 1
    /// (`research/evidence/material-survey.txt`).
    pub env_factor: Option<[f32; 3]>,
    /// The two `fresnel` parameters of reflecting effects.
    pub fresnel: Option<[f32; 2]>,
    /// `.tobj` path of the reflection cube map (`texture_reflection`).
    pub reflection_texture: Option<String>,
    /// Weight of the scene's additional ambient colour
    /// (`additional_ambient`; dif-spec.md §3.1).
    pub additional_ambient: Option<f32>,
    /// `.tobj` path of the tangent-space normal map (`texture_nmap`), for the
    /// `tsnmap`, `tsnmapuv` and `tsnmapuv2` flavors.
    pub normal_texture: Option<String>,
    /// `.tobj` path of the detail normal map of `tsnmapuv2`
    /// (`texture_nmap_detail`).
    pub normal_detail_texture: Option<String>,
    /// `.tobj` path of the baked shading (`texture_mult`) of the `mult`
    /// bases and the scene shaders.
    pub mult_texture: Option<String>,
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
            specular: None,
            shininess: None,
            env_factor: None,
            fresnel: None,
            reflection_texture: None,
            additional_ambient: None,
            normal_texture: None,
            normal_detail_texture: None,
            mult_texture: None,
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
        let floats = |key: &str| material.attribute(key).and_then(MatValue::as_floats);
        let texture = |slot: &str| {
            material
                .texture(slot)
                .and_then(|t| t.source.as_deref())
                .map(|source| resolve_relative(path, source))
        };
        let rgb = |key: &str| floats(key).and_then(|v| <[f32; 3]>::try_from(v.get(..3)?).ok());
        Self {
            path: path.to_owned(),
            effect: material.effect.clone(),
            blend,
            diffuse: rgb("diffuse").unwrap_or([1.0; 3]),
            base_texture: texture("texture_base"),
            paintable: has("truckpaint"),
            emissive: has("lamp") || has("lum") || has("light"),
            decal: has("decal") || has("over"),
            specular: rgb("specular"),
            shininess: floats("shininess").and_then(|v| v.first().copied()),
            env_factor: rgb("env_factor"),
            fresnel: floats("fresnel").and_then(|v| <[f32; 2]>::try_from(v.get(..2)?).ok()),
            reflection_texture: texture("texture_reflection"),
            additional_ambient: floats("additional_ambient").and_then(|v| v.first().copied()),
            normal_texture: texture("texture_nmap"),
            normal_detail_texture: texture("texture_nmap_detail"),
            mult_texture: texture("texture_mult"),
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
    /// For a `truckpaint` piece, the UV set its paint samples (masks,
    /// flakes): see [`paint_uv_set`].
    pub paint_uvs: Option<Vec<[f32; 2]>>,
    /// The UV set `alternate_uvset` paint jobs sample, when it differs from
    /// `paint_uvs`.
    pub alternate_paint_uvs: Option<Vec<[f32; 2]>>,
    /// Vertex colours as RGBA in 0–1, if present: each stored byte / 255,
    /// the lowest byte red (Hypothesis: truck paint pieces are grey, so the
    /// order is untested; `paint_uv_survey`).
    pub colors: Option<Vec<[f32; 4]>>,
    /// The UV set the piece routes to UV slot 6, "UV Normal", which the
    /// `tsnmapuv` flavors sample their normal map with. Only those pieces
    /// route slot 6 (Confirmed); that the normal map reads it is a
    /// Hypothesis (`research/evidence/pmg-uv-routing.txt`).
    pub normal_uvs: Option<Vec<[f32; 2]>>,
    /// The UV set the piece routes to UV slot 1, which `texture_mult` (the
    /// `mult` bases' baked occlusion) samples
    /// (`docs/app/shader-specs/dif-spec-mult.md`).
    pub mult_uvs: Option<Vec<[f32; 2]>>,
    /// Tangents as stored, `(x, y, z, w)` with `w = ±1` the handedness, if
    /// the piece has a tangent stream. Reversing the winding (see `indices`)
    /// leaves them unchanged.
    pub tangents: Option<Vec<[f32; 4]>>,
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

/// The UV slot of the "UV Normal" stream (`docs/formats/models.md`, piece +8).
const NORMAL_UV_SLOT: usize = 6;
/// The UV slot `texture_mult` samples.
const MULT_UV_SLOT: usize = 1;

/// The PMG UV set, counted from 0, that truck paint samples on a mesh with
/// `sets` UV sets: set 1, or set 2 for `alternate_uvset` paint jobs
/// (Confirmed on screen: an alternate-UV decal on the Actros 2014 cab lands
/// on the door only from set 2; `docs/app/shader-specs/truckpaint.md` §2).
/// A mesh without that set falls back to its last one (Hypothesis: the
/// Scania S cab has only set 0 and its masks line up).
pub fn paint_uv_set(sets: usize, alternate: bool) -> Option<usize> {
    let wanted = if alternate { 2 } else { 1 };
    sets.checked_sub(1).map(|last| last.min(wanted))
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
            let painted = materials.get(material).is_some_and(|m| m.paintable);
            let sets = if painted { piece.uv_sets.len() } else { 0 };
            let paint_set = paint_uv_set(sets, false);
            let alternate_set = paint_uv_set(sets, true).filter(|&set| Some(set) != paint_set);
            let slot_uvs = |slot| {
                piece
                    .uv_set_for_slot(slot)
                    .and_then(|set| piece.uv_sets.get(set).cloned())
            };
            pieces.push(MeshPiece {
                material,
                positions: piece.positions.clone(),
                normals: piece.normals.clone(),
                uvs: piece.uv_sets.first().cloned(),
                paint_uvs: paint_set.and_then(|set| piece.uv_sets.get(set).cloned()),
                alternate_paint_uvs: alternate_set.and_then(|set| piece.uv_sets.get(set).cloned()),
                colors: vertex_colors(&piece.colors),
                normal_uvs: slot_uvs(NORMAL_UV_SLOT),
                mult_uvs: slot_uvs(MULT_UV_SLOT),
                tangents: piece.tangents.clone(),
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

/// Packed RGBA8 vertex colours as floats; `None` for an absent stream.
fn vertex_colors(packed: &[u32]) -> Option<Vec<[f32; 4]>> {
    (!packed.is_empty()).then(|| {
        packed
            .iter()
            .map(|c| c.to_le_bytes().map(|byte| f32::from(byte) / 255.0))
            .collect()
    })
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
    /// R9G9B9E5_SHAREDEXP: linear HDR, three 9-bit mantissas sharing a
    /// 5-bit exponent (skyboxes, HDR UI images).
    Rgb9e5,
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

/// A cube texture ready for upload: six faces in their stored order, each a
/// tightly packed mip chain. The order is taken to be +x, −x, +y, −y, +z, −z
/// (the DDS convention; y Confirmed, x/z mirroring Unknown, see
/// `docs/formats/textures.md`).
#[derive(Clone, Debug, PartialEq)]
pub struct CubeTextureData {
    pub format: TextureFormat,
    pub srgb: bool,
    pub faces: Vec<Vec<MipLevel>>,
}

/// Loads `.tobj` texture `path` and converts it for upload.
pub fn load_texture(fs: &GameFs, path: &str) -> Result<TextureData> {
    let packed = fs.vfs().read_texture(path.trim_start_matches('/'))?;
    convert_texture(path, &packed)
}

/// Loads the six-face cube texture `path` (e.g. a material's
/// `texture_reflection`) and converts it for upload.
pub fn load_cube_texture(fs: &GameFs, path: &str) -> Result<CubeTextureData> {
    let packed = fs.vfs().read_texture(path.trim_start_matches('/'))?;
    convert_cube(path, &packed)
}

fn convert_cube(path: &str, packed: &PackedTexture) -> Result<CubeTextureData> {
    if !packed.image.is_cube || packed.image.face_count != 6 {
        return Err(GameDataError::Definition {
            path: path.to_owned(),
            message: format!("not a cube texture ({} faces)", packed.image.face_count),
        });
    }
    let (format, srgb, faces) = convert_faces(path, packed)?;
    Ok(CubeTextureData {
        format,
        srgb,
        faces,
    })
}

fn convert_texture(path: &str, packed: &PackedTexture) -> Result<TextureData> {
    let (format, srgb, faces) = convert_faces(path, packed)?;
    let repeat = |mode: specsheet_archive::AddressMode| {
        matches!(mode, specsheet_archive::AddressMode::Repeat)
    };
    Ok(TextureData {
        format,
        srgb,
        mips: faces.into_iter().next().unwrap_or_default(),
        repeat_u: repeat(packed.sample.address_u),
        repeat_v: repeat(packed.sample.address_v),
    })
}

/// The upload format, its sRGB flag and the unpacked mip chain of every face.
fn convert_faces(
    path: &str,
    packed: &PackedTexture,
) -> Result<(TextureFormat, bool, Vec<Vec<MipLevel>>)> {
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
        67 => (TextureFormat::Rgb9e5, false, false),
        other => return Err(format_error(format!("unsupported DXGI format {other}"))),
    };
    let x_channel_is_opaque = matches!(dxgi, 88 | 92 | 93);
    if pixel_layout(dxgi).is_none() {
        return Err(format_error(format!("no layout for DXGI format {dxgi}")));
    }
    let mut faces = vec![Vec::new(); packed.image.face_count as usize];
    for surface in packed.unpack().map_err(format_error)? {
        let mut data = surface.data;
        if swizzle_bgra {
            for pixel in data.chunks_exact_mut(4) {
                pixel.swap(0, 2);
                if x_channel_is_opaque {
                    pixel[3] = 255;
                }
            }
        }
        let face = faces
            .get_mut(surface.face as usize)
            .ok_or_else(|| format_error(format!("surface of face {}", surface.face)))?;
        face.push(MipLevel {
            width: surface.width,
            height: surface.height,
            data,
        });
    }
    Ok((format, srgb, faces))
}

/// Bytes per block or pixel of an upload format, for consumers that
/// validate sizes.
pub fn texture_block_bytes(format: TextureFormat) -> (u32, PixelLayout) {
    match format {
        TextureFormat::Bc1 | TextureFormat::Bc4 => (8, PixelLayout::Block { bytes: 8 }),
        TextureFormat::Bc2 | TextureFormat::Bc3 | TextureFormat::Bc5 => {
            (16, PixelLayout::Block { bytes: 16 })
        }
        TextureFormat::Rgba8 | TextureFormat::Rgb9e5 => {
            (4, PixelLayout::Linear { bytes_per_pixel: 4 })
        }
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
    fn paint_samples_uv_set_one_or_two_counted_from_zero() {
        // The Actros 2014 cab's paint piece has 3 sets: an alternate-UV
        // decal lands on the door only from set 2.
        assert_eq!(paint_uv_set(3, false), Some(1));
        assert_eq!(paint_uv_set(3, true), Some(2));
        // A mesh without the set falls back to its last one (Hypothesis).
        assert_eq!(paint_uv_set(2, true), Some(1));
        assert_eq!(paint_uv_set(1, false), Some(0));
        assert_eq!(paint_uv_set(0, false), None);
    }

    #[test]
    fn vertex_colours_decode_lowest_byte_first() {
        assert_eq!(vertex_colors(&[]), None);
        let colors = vertex_colors(&[0xff00_80ff]).expect("colours");
        assert_eq!(colors, [[1.0, 128.0 / 255.0, 0.0, 1.0]]);
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

    /// A packed 2x2 RGBA8 texture with one mip per face; face `f` is filled
    /// with byte `f`.
    fn packed(faces: u32) -> PackedTexture {
        use specsheet_archive::{AddressMode, ImageMeta, SampleMeta};
        PackedTexture {
            image: ImageMeta {
                width: 2,
                height: 2,
                mip_count: 1,
                dxgi_format: 28,
                is_cube: faces == 6,
                face_count: faces,
                pitch_alignment: 1,
                image_alignment: 1,
            },
            sample: SampleMeta {
                mag_linear: true,
                min_linear: true,
                address_u: AddressMode::ClampToEdge,
                address_v: AddressMode::ClampToEdge,
            },
            surfaces: (0..faces as u8).flat_map(|f| [f; 16]).collect(),
        }
    }

    #[test]
    fn converts_all_six_faces_of_a_cube() {
        let cube = convert_cube("/c.tobj", &packed(6)).expect("cube");
        assert_eq!(cube.faces.len(), 6);
        for (f, mips) in cube.faces.iter().enumerate() {
            assert_eq!(mips.len(), 1);
            assert_eq!((mips[0].width, mips[0].height), (2, 2));
            assert!(mips[0].data.iter().all(|&b| usize::from(b) == f));
        }
        assert!(convert_cube("/flat.tobj", &packed(1)).is_err());
        // A 2D load of a cube keeps face 0, as before.
        let flat = convert_texture("/c.tobj", &packed(6)).expect("2d");
        assert!(flat.mips[0].data.iter().all(|&b| b == 0));
    }

    #[test]
    fn carries_normal_map_and_baked_shading_paths() {
        let source = r#"effect : "eut2.dif.spec.add.env.tsnmapuv.rfx" {
            texture : "texture_base" { source : "metal.tobj" }
            texture : "texture_nmap" { source : "/vehicle/truck/x/metal_n.tobj" }
            texture : "texture_nmap_detail" { source : "grain_n.tobj" }
            texture : "texture_mult" { source : "ao.tobj" }
        }"#;
        let mat = Material::parse(source.as_bytes()).expect("parse");
        let m = MaterialInfo::from_material("/vehicle/truck/x/metal.mat", &mat);
        assert_eq!(
            m.normal_texture.as_deref(),
            Some("/vehicle/truck/x/metal_n.tobj")
        );
        assert_eq!(
            m.normal_detail_texture.as_deref(),
            Some("/vehicle/truck/x/grain_n.tobj")
        );
        assert_eq!(m.mult_texture.as_deref(), Some("/vehicle/truck/x/ao.tobj"));
        let plain = material("eut2.dif.rfx");
        assert_eq!(
            (
                plain.normal_texture,
                plain.normal_detail_texture,
                plain.mult_texture
            ),
            (None, None, None)
        );
    }

    #[test]
    fn hdr_cubes_upload_as_rgb9e5_bytes_unchanged() {
        let mut hdr = packed(6);
        hdr.image.dxgi_format = 67;
        let cube = convert_cube("/sky.tobj", &hdr).expect("rgb9e5 cube");
        assert_eq!((cube.format, cube.srgb), (TextureFormat::Rgb9e5, false));
        assert!(cube.faces[5][0].data.iter().all(|&b| b == 5));
    }

    #[test]
    fn carries_shine_and_reflection_attributes() {
        // A typical chrome (`research/evidence/material-survey.txt`).
        let source = r#"effect : "eut2.dif.spec.weight.add.env.rfx" {
            diffuse : { 0.214041 , 0.214041 , 0.214041 }
            additional_ambient : 0.5
            env_factor : { 0.977402 , 0.977402 , 0.977402 }
            fresnel : { 1 , 1 }
            shininess : 70
            specular : { 1 , 1 , 1 }
            texture : "texture_reflection" { source : "/material/environment/vehicle_reflection.tobj" }
        }"#;
        let mat = Material::parse(source.as_bytes()).expect("parse");
        let m = MaterialInfo::from_material("/vehicle/truck/x/chrome.mat", &mat);
        assert_eq!(m.specular, Some([1.0; 3]));
        assert_eq!(m.shininess, Some(70.0));
        assert_eq!(m.env_factor, Some([0.977402; 3]));
        assert_eq!(m.fresnel, Some([1.0, 1.0]));
        assert_eq!(m.additional_ambient, Some(0.5));
        assert_eq!(
            m.reflection_texture.as_deref(),
            Some("/material/environment/vehicle_reflection.tobj")
        );

        let plain = material("eut2.dif.rfx");
        assert_eq!(
            (plain.specular, plain.shininess, plain.env_factor),
            (None, None, None)
        );
        assert_eq!(plain.reflection_texture, None);
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
