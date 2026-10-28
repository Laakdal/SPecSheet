//! Converts `specsheet-data` meshes, materials and textures into Bevy assets.
//!
//! Game files are hostile input and wgpu panics on malformed uploads, so
//! every texture and mesh is validated here first; anything that does not
//! fit is skipped with a warning (see `docs/RENDERING.md`).

use std::collections::HashMap;
use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{
    Extent3d, TextureDimension, TextureFormat as GpuFormat, TextureViewDescriptor,
    TextureViewDimension,
};
use specsheet_data::render::{
    build_mesh, load_texture, texture_block_bytes, BlendMode, CubeTextureData, MaterialInfo,
    MeshData, MeshPiece, MipLevel, TextureData, TextureFormat,
};
use specsheet_data::session::GameSession;

use crate::render::PaintSurface;

/// One model as the viewer requests it. `parts` limits the model to the
/// named parts (empty: all), as scene models do.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ModelKey {
    pub path: String,
    pub look: Option<String>,
    pub variant: Option<String>,
    pub parts: Vec<String>,
}

/// CPU-side model data, built on a worker thread.
pub struct ModelPayload {
    pub mesh: MeshData,
    pub textures: Vec<(String, Result<TextureData, String>)>,
    pub warnings: Vec<String>,
}

/// Reads and converts a model with the textures its materials use.
pub fn load_model(game: &GameSession, key: &ModelKey) -> Result<ModelPayload, String> {
    let model = game
        .models()
        .load(game.fs(), &key.path)
        .map_err(|e| e.to_string())?;
    let (mesh, warnings) = build_mesh(
        game.fs(),
        &model,
        key.look.as_deref(),
        key.variant.as_deref(),
        &key.parts,
    );
    let mut paths: Vec<&String> = mesh
        .materials
        .iter()
        .filter_map(|m| m.base_texture.as_ref())
        .collect();
    paths.sort();
    paths.dedup();
    let textures = paths
        .into_iter()
        .map(|path| {
            let texture = load_texture(game.fs(), path).map_err(|e| e.to_string());
            (path.clone(), texture)
        })
        .collect();
    Ok(ModelPayload {
        mesh,
        textures,
        warnings,
    })
}

/// A model uploaded to the GPU; instances share its meshes and materials.
pub struct GpuModel {
    pub pieces: Vec<(Handle<Mesh>, Handle<StandardMaterial>)>,
    /// Per piece: for a `truckpaint` material, what its `.mat` gives the
    /// paint; such pieces take the paint of the placement they belong to.
    pub painted: Vec<Option<PaintPiece>>,
    /// The reflection cube (`texture_reflection`) of every reflective
    /// material, once per material; the viewer uses the most common one as
    /// its environment map.
    pub reflections: Vec<String>,
    pub aabb_min: Vec3,
    pub aabb_max: Vec3,
}

/// A `truckpaint` piece of a [`GpuModel`].
pub struct PaintPiece {
    pub surface: PaintSurface,
    /// The piece's mesh for `alternate_uvset` paint jobs, whose paint UV
    /// (`UV_1`) is another set; `None` when it is the same mesh.
    pub alternate_mesh: Option<Handle<Mesh>>,
}

/// Bevy asset stores used while uploading.
pub struct Stores<'a> {
    pub meshes: &'a mut Assets<Mesh>,
    pub materials: &'a mut Assets<StandardMaterial>,
    pub images: &'a mut Assets<Image>,
    /// Converted textures by game path; `None` when one could not be used.
    pub textures: &'a mut HashMap<String, Option<Handle<Image>>>,
    pub bc_supported: bool,
}

/// Uploads `payload`, reusing textures converted for earlier models.
/// Returns the model and the problems found on the way.
pub fn upload(payload: ModelPayload, stores: &mut Stores<'_>) -> (Arc<GpuModel>, Vec<String>) {
    let ModelPayload {
        mesh,
        textures,
        mut warnings,
    } = payload;
    for (path, texture) in textures {
        if stores.textures.contains_key(&path) {
            continue;
        }
        let handle = match texture.and_then(|data| texture_image(data, stores.bc_supported)) {
            Ok(image) => Some(stores.images.add(image)),
            Err(error) => {
                warnings.push(format!("{path}: {error}"));
                None
            }
        };
        stores.textures.insert(path, handle);
    }
    let materials: Vec<Handle<StandardMaterial>> = mesh
        .materials
        .iter()
        .map(|info| {
            let texture = info
                .base_texture
                .as_ref()
                .and_then(|path| stores.textures.get(path).cloned().flatten());
            stores.materials.add(material(info, texture))
        })
        .collect();
    let mut pieces = Vec::new();
    let mut painted = Vec::new();
    for (index, piece) in mesh.pieces.into_iter().enumerate() {
        let Some(material) = materials.get(piece.material) else {
            warnings.push(format!("{}: piece {index} has no material", mesh.path));
            continue;
        };
        let surface = mesh
            .materials
            .get(piece.material)
            .and_then(PaintSurface::of);
        let alternate = match (&surface, &piece.alternate_paint_uvs) {
            (Some(_), Some(uvs)) => Some(MeshPiece {
                paint_uvs: Some(uvs.clone()),
                ..piece.clone()
            }),
            _ => None,
        };
        match piece_mesh(piece, surface.is_some()).and_then(|m| {
            let alternate = alternate.map(|p| piece_mesh(p, true)).transpose()?;
            Ok((m, alternate))
        }) {
            Ok((m, alternate)) => {
                pieces.push((stores.meshes.add(m), material.clone()));
                painted.push(surface.map(|surface| PaintPiece {
                    surface,
                    alternate_mesh: alternate.map(|m| stores.meshes.add(m)),
                }));
            }
            Err(error) => warnings.push(format!("{}: piece {index}: {error}", mesh.path)),
        }
    }
    let reflections = mesh
        .materials
        .iter()
        .filter(|info| reflective_surface(info).is_some())
        .filter_map(|info| info.reflection_texture.clone())
        .collect();
    let model = GpuModel {
        pieces,
        painted,
        reflections,
        aabb_min: Vec3::from(mesh.aabb_min),
        aabb_max: Vec3::from(mesh.aabb_max),
    };
    (Arc::new(model), warnings)
}

/// The GPU mesh of `piece`. Paint pieces also carry their paint UV set as
/// `UV_1` and their vertex colours, doubled: the paint multiplies by twice
/// the colour, so 0.5 is neutral (truckpaint-lighting.md §2). Other
/// materials do not use them yet.
fn piece_mesh(piece: MeshPiece, painted: bool) -> Result<Mesh, String> {
    let count = piece.positions.len();
    if piece.normals.len() != count {
        return Err("normal count differs from position count".to_owned());
    }
    if piece.colors.as_ref().is_some_and(|c| c.len() != count) {
        return Err("colour count differs from position count".to_owned());
    }
    if [&piece.uvs, &piece.paint_uvs]
        .into_iter()
        .any(|uvs| uvs.as_ref().is_some_and(|uvs| uvs.len() != count))
    {
        return Err("UV count differs from position count".to_owned());
    }
    if !piece.indices.len().is_multiple_of(3) || piece.indices.iter().any(|&i| i as usize >= count)
    {
        return Err("index out of range or incomplete triangle".to_owned());
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, piece.positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, piece.normals);
    if let Some(uvs) = piece.uvs {
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    }
    if let Some(uvs) = piece.paint_uvs.filter(|_| painted) {
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, uvs);
    }
    if let Some(colors) = piece.colors.filter(|_| painted) {
        let doubled: Vec<[f32; 4]> = colors
            .into_iter()
            .map(|[r, g, b, a]| [2.0 * r, 2.0 * g, 2.0 * b, a])
            .collect();
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, doubled);
    }
    mesh.insert_indices(Indices::U32(piece.indices));
    Ok(mesh)
}

/// Opacity of blended materials without a texture alpha that says
/// otherwise; three.js used the same values (`docs/RENDERING.md`).
const GLASS_OPACITY: f32 = 0.35;
const WATER_OPACITY: f32 = 0.6;
/// Emissive strength of lamp materials, in Bevy's luminance units.
const EMISSIVE_LUMINANCE: f32 = 40.0;
/// Depth bias that keeps decals in front of the surface they sit on.
const DECAL_DEPTH_BIAS: f32 = 8.0;
/// Below this perceptual roughness Bevy's specular highlights alias.
const MIN_ROUGHNESS: f32 = 0.089;

/// Base colour and metalness of a reflecting material (an `env` effect with
/// an `env_factor`). The game adds the environment reflection on top of a
/// dark diffuse; chrome is diffuse 0.21 with a factor of 0.98
/// (`research/evidence/material-survey.txt`). Here the factor's luminance
/// becomes metalness, and the base colour blends from the diffuse towards
/// the factor's colour by the same amount, so chrome reads as bright metal.
fn reflective_surface(info: &MaterialInfo) -> Option<([f32; 3], f32)> {
    if info.paintable
        || info.emissive
        || matches!(info.blend, BlendMode::Blend | BlendMode::Additive)
    {
        return None;
    }
    let factor = info.env_factor?;
    let strength = (0.2126 * factor[0] + 0.7152 * factor[1] + 0.0722 * factor[2]).clamp(0.0, 1.0);
    let base =
        std::array::from_fn(|i| info.diffuse[i] * (1.0 - strength) + factor[i].min(1.0) * strength);
    Some((base, strength))
}

/// Blinn-Phong exponent to perceptual roughness: the GGX alpha that matches
/// exponent `n` is `sqrt(2 / (n + 2))`, and Bevy's perceptual roughness is
/// the square root of alpha.
fn roughness_from_shininess(n: f32) -> f32 {
    (2.0 / (n.max(0.0) + 2.0))
        .sqrt()
        .sqrt()
        .clamp(MIN_ROUGHNESS, 1.0)
}

/// The PBR stand-in for a game material. This is an approximation, not the
/// game's shaders.
pub fn material(info: &MaterialInfo, texture: Option<Handle<Image>>) -> StandardMaterial {
    let reflective = reflective_surface(info);
    let [r, g, b] = reflective.map_or(info.diffuse, |(base, _)| base);
    let tokens: Vec<&str> = info.effect.split('.').collect();
    let opacity = if tokens.contains(&"glass") {
        GLASS_OPACITY
    } else if tokens.contains(&"water") {
        WATER_OPACITY
    } else {
        1.0
    };
    let alpha_mode = match info.blend {
        BlendMode::AlphaTest => AlphaMode::Mask(0.5),
        BlendMode::Blend => AlphaMode::Blend,
        BlendMode::Additive => AlphaMode::Add,
        BlendMode::Opaque | BlendMode::Hidden => AlphaMode::Opaque,
    };
    StandardMaterial {
        // `.mat` colours are linear, like paint colours.
        base_color: Color::linear_rgba(r, g, b, opacity),
        emissive: if info.emissive {
            LinearRgba::rgb(1.0, 1.0, 1.0) * EMISSIVE_LUMINANCE
        } else {
            LinearRgba::BLACK
        },
        emissive_texture: texture.clone().filter(|_| info.emissive),
        base_color_texture: texture,
        // Paint sets its own in the truckpaint shader.
        metallic: reflective.map_or(0.1, |(_, metalness)| metalness),
        perceptual_roughness: match (reflective, info.shininess) {
            (Some(_), Some(n)) => roughness_from_shininess(n),
            _ => 0.6,
        },
        alpha_mode,
        depth_bias: if info.decal { DECAL_DEPTH_BIAS } else { 0.0 },
        ..default()
    }
}

/// Largest texture side accepted (the wgpu limit on desktop adapters).
const MAX_TEXTURE_SIZE: u32 = 16384;
/// Largest cube side Bevy's environment map generator accepts.
const MAX_CUBE_SIZE: u32 = 8192;

/// Builds a GPU image from a converted game texture, checking every size
/// wgpu would otherwise panic on.
pub fn texture_image(data: TextureData, bc_supported: bool) -> Result<Image, String> {
    check_bc(data.format, bc_supported)?;
    let (width, height, bytes) = mip_chain(data.format, &data.mips)?;
    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        bytes,
        gpu_format(data.format, data.srgb),
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.mip_level_count = data.mips.len() as u32;
    let address = |repeat: bool| {
        if repeat {
            ImageAddressMode::Repeat
        } else {
            ImageAddressMode::ClampToEdge
        }
    };
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: address(data.repeat_u),
        address_mode_v: address(data.repeat_v),
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 4,
        ..default()
    });
    Ok(image)
}

/// Builds the environment cube from a game reflection cube. Bevy's
/// generated environment map panics unless the cube is square with a
/// power-of-two side of at most 8192, so that is checked here, along with
/// every face carrying the same mip chain.
pub fn cube_image(data: CubeTextureData, bc_supported: bool) -> Result<Image, String> {
    check_bc(data.format, bc_supported)?;
    if data.faces.len() != 6 {
        return Err(format!("cube has {} faces", data.faces.len()));
    }
    let mut bytes = Vec::new();
    let mut layout = None;
    for (index, face) in data.faces.iter().enumerate() {
        let (width, height, face_bytes) =
            mip_chain(data.format, face).map_err(|e| format!("face {index}: {e}"))?;
        if width != height || !width.is_power_of_two() || width > MAX_CUBE_SIZE {
            return Err(format!(
                "face {index} is {width}x{height}, not a power-of-two square"
            ));
        }
        match layout {
            None => layout = Some((width, face.len())),
            Some(first) if first != (width, face.len()) => {
                return Err(format!("face {index} differs from face 0"));
            }
            Some(_) => {}
        }
        bytes.extend(face_bytes);
    }
    let Some((side, mip_count)) = layout else {
        return Err("cube has no faces".to_owned());
    };
    let mut image = Image::new(
        Extent3d {
            width: side,
            height: side,
            depth_or_array_layers: 6,
        },
        TextureDimension::D2,
        bytes,
        gpu_format(data.format, data.srgb),
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.mip_level_count = mip_count as u32;
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    });
    Ok(image)
}

/// Mean linear luminance of a cube, read from the smallest mip of each face
/// (for BC formats, the mean of the block's two endpoint colours). Used to
/// keep the scene's exposure when the cube replaces the gradient. `None` for
/// formats it cannot read.
pub fn cube_mean_luminance(data: &CubeTextureData) -> Option<f32> {
    let mut sum = 0.0;
    for face in &data.faces {
        let data_bytes = &face.last()?.data;
        let rgb = match data.format {
            TextureFormat::Bc1 => endpoint_mean(data_bytes.get(..4)?),
            TextureFormat::Bc2 | TextureFormat::Bc3 => endpoint_mean(data_bytes.get(8..12)?),
            TextureFormat::Rgba8 => {
                let pixels = data_bytes.len() / 4;
                if pixels == 0 {
                    return None;
                }
                let mut rgb = [0.0; 3];
                for pixel in data_bytes.chunks_exact(4) {
                    for (channel, value) in rgb.iter_mut().zip(pixel) {
                        *channel += f32::from(*value) / 255.0 / pixels as f32;
                    }
                }
                rgb
            }
            _ => return None,
        };
        let [r, g, b] = if data.srgb {
            rgb.map(srgb_to_linear)
        } else {
            rgb
        };
        sum += 0.2126 * r + 0.7152 * g + 0.0722 * b;
    }
    (!data.faces.is_empty()).then(|| sum / data.faces.len() as f32)
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

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn is_block(format: TextureFormat) -> bool {
    matches!(
        format,
        TextureFormat::Bc1
            | TextureFormat::Bc2
            | TextureFormat::Bc3
            | TextureFormat::Bc4
            | TextureFormat::Bc5
    )
}

fn check_bc(format: TextureFormat, bc_supported: bool) -> Result<(), String> {
    if is_block(format) && !bc_supported {
        return Err("GPU lacks BC texture compression".to_owned());
    }
    Ok(())
}

/// Checks a mip chain against what wgpu accepts and concatenates it.
/// Returns the base width, height and the bytes.
fn mip_chain(format: TextureFormat, mips: &[MipLevel]) -> Result<(u32, u32, Vec<u8>), String> {
    let block = is_block(format);
    let (unit_bytes, _) = texture_block_bytes(format);
    let base = mips.first().ok_or("texture has no surfaces")?;
    let (width, height) = (base.width, base.height);
    if width == 0 || height == 0 || width > MAX_TEXTURE_SIZE || height > MAX_TEXTURE_SIZE {
        return Err(format!("unsupported size {width}x{height}"));
    }
    if block && (width % 4 != 0 || height % 4 != 0) {
        return Err(format!(
            "BC texture size {width}x{height} is not a multiple of 4"
        ));
    }
    let max_mips = 32 - width.max(height).leading_zeros();
    if mips.len() > max_mips as usize {
        return Err(format!(
            "{} mips exceed the chain of {max_mips}",
            mips.len()
        ));
    }
    let mut bytes = Vec::new();
    for (level, mip) in mips.iter().enumerate() {
        let (w, h) = ((width >> level).max(1), (height >> level).max(1));
        if (mip.width, mip.height) != (w, h) {
            return Err(format!(
                "mip {level} is {}x{}, expected {w}x{h}",
                mip.width, mip.height
            ));
        }
        let expected = if block {
            w.div_ceil(4) as usize * h.div_ceil(4) as usize * unit_bytes as usize
        } else {
            w as usize * h as usize * unit_bytes as usize
        };
        if mip.data.len() != expected {
            return Err(format!(
                "mip {level} has {} bytes, expected {expected}",
                mip.data.len()
            ));
        }
        bytes.extend_from_slice(&mip.data);
    }
    Ok((width, height, bytes))
}

fn gpu_format(format: TextureFormat, srgb: bool) -> GpuFormat {
    match (format, srgb) {
        (TextureFormat::Bc1, false) => GpuFormat::Bc1RgbaUnorm,
        (TextureFormat::Bc1, true) => GpuFormat::Bc1RgbaUnormSrgb,
        (TextureFormat::Bc2, false) => GpuFormat::Bc2RgbaUnorm,
        (TextureFormat::Bc2, true) => GpuFormat::Bc2RgbaUnormSrgb,
        (TextureFormat::Bc3, false) => GpuFormat::Bc3RgbaUnorm,
        (TextureFormat::Bc3, true) => GpuFormat::Bc3RgbaUnormSrgb,
        (TextureFormat::Bc4, _) => GpuFormat::Bc4RUnorm,
        (TextureFormat::Bc5, _) => GpuFormat::Bc5RgUnorm,
        (TextureFormat::Rgba8, false) => GpuFormat::Rgba8Unorm,
        (TextureFormat::Rgba8, true) => GpuFormat::Rgba8UnormSrgb,
        (TextureFormat::R8, _) => GpuFormat::R8Unorm,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bc1(width: u32, height: u32, mips: usize) -> TextureData {
        TextureData {
            format: TextureFormat::Bc1,
            srgb: true,
            mips: (0..mips)
                .map(|level| {
                    let (w, h) = ((width >> level).max(1), (height >> level).max(1));
                    MipLevel {
                        width: w,
                        height: h,
                        data: vec![0; (w.div_ceil(4) * h.div_ceil(4) * 8) as usize],
                    }
                })
                .collect(),
            repeat_u: true,
            repeat_v: false,
        }
    }

    #[test]
    fn measures_a_cube_from_its_smallest_mips() {
        // One 4x4 BC1 block per face with both endpoints pure white.
        let white = vec![MipLevel {
            width: 4,
            height: 4,
            data: vec![0xff, 0xff, 0xff, 0xff, 0, 0, 0, 0],
        }];
        let cube = CubeTextureData {
            format: TextureFormat::Bc1,
            srgb: true,
            faces: vec![white; 6],
        };
        let mean = cube_mean_luminance(&cube).expect("readable");
        assert!((mean - 1.0).abs() < 1e-4);
    }

    #[test]
    fn builds_a_cube_and_rejects_malformed_ones() {
        let face = |side: u32| bc1(side, side, 3).mips;
        let cube = |faces: Vec<Vec<MipLevel>>| CubeTextureData {
            format: TextureFormat::Bc1,
            srgb: true,
            faces,
        };
        let image = cube_image(cube(vec![face(16); 6]), true).expect("valid cube");
        assert_eq!(image.texture_descriptor.size.depth_or_array_layers, 6);
        assert_eq!(image.texture_descriptor.mip_level_count, 3);

        assert!(cube_image(cube(vec![face(16); 5]), true).is_err());
        let mut mixed = vec![face(16); 6];
        mixed[3] = face(8);
        assert!(cube_image(cube(mixed), true).is_err());
        // Bevy's generator panics on a side that is not a power of two.
        assert!(cube_image(cube(vec![bc1(12, 12, 1).mips; 6]), true).is_err());
        assert!(cube_image(cube(vec![face(16); 6]), false).is_err());
    }

    #[test]
    fn accepts_a_full_bc1_chain() {
        let image = texture_image(bc1(16, 8, 5), true).expect("valid");
        assert_eq!(image.texture_descriptor.mip_level_count, 5);
        assert_eq!(image.texture_descriptor.format, GpuFormat::Bc1RgbaUnormSrgb);
        // 16x8 → 8 blocks, 8x4 → 2, 4x2 → 1, 2x1 → 1, 1x1 → 1.
        assert_eq!(image.data.map(|d| d.len()), Some((8 + 2 + 1 + 1 + 1) * 8));
    }

    #[test]
    fn rejects_textures_wgpu_would_panic_on() {
        assert!(
            texture_image(bc1(6, 8, 1), true).is_err(),
            "not block aligned"
        );
        assert!(
            texture_image(bc1(16, 8, 1), false).is_err(),
            "no BC support"
        );
        assert!(texture_image(bc1(4, 4, 4), true).is_err(), "too many mips");
        let mut short = bc1(16, 16, 2);
        short.mips[1].data.pop();
        assert!(texture_image(short, true).is_err(), "truncated mip");
        let mut wrong = bc1(16, 16, 2);
        wrong.mips[1].width = 4;
        assert!(texture_image(wrong, true).is_err(), "mip size");
        let empty = TextureData {
            mips: Vec::new(),
            ..bc1(4, 4, 1)
        };
        assert!(texture_image(empty, true).is_err(), "no surfaces");
    }

    #[test]
    fn rejects_meshes_with_mismatched_attributes() {
        let piece = |indices: Vec<u32>, normals: usize| MeshPiece {
            material: 0,
            positions: vec![[0.0; 3]; 3],
            normals: vec![[0.0, 1.0, 0.0]; normals],
            uvs: None,
            paint_uvs: None,
            alternate_paint_uvs: None,
            colors: None,
            indices,
        };
        assert!(piece_mesh(piece(vec![0, 1, 2], 3), false).is_ok());
        assert!(piece_mesh(piece(vec![0, 1, 3], 3), false).is_err());
        assert!(piece_mesh(piece(vec![0, 1], 3), false).is_err());
        assert!(piece_mesh(piece(vec![0, 1, 2], 2), false).is_err());
        let short_colours = MeshPiece {
            colors: Some(vec![[0.5; 4]; 2]),
            ..piece(vec![0, 1, 2], 3)
        };
        assert!(piece_mesh(short_colours, true).is_err());
        let short_paint_uvs = MeshPiece {
            paint_uvs: Some(vec![[0.0; 2]; 2]),
            ..piece(vec![0, 1, 2], 3)
        };
        assert!(piece_mesh(short_paint_uvs, true).is_err());
    }

    #[test]
    fn maps_blend_modes_and_glass_opacity() {
        let info = |effect: &str, blend| MaterialInfo {
            path: String::new(),
            effect: effect.to_owned(),
            blend,
            diffuse: [1.0; 3],
            base_texture: None,
            paintable: false,
            emissive: false,
            decal: false,
            specular: None,
            shininess: None,
            env_factor: None,
            fresnel: None,
            reflection_texture: None,
        };
        let glass = material(&info("eut2.glass", BlendMode::Blend), None);
        assert_eq!(glass.alpha_mode, AlphaMode::Blend);
        assert_eq!(glass.base_color.alpha(), GLASS_OPACITY);
        let cutout = material(&info("eut2.dif.a", BlendMode::AlphaTest), None);
        assert_eq!(cutout.alpha_mode, AlphaMode::Mask(0.5));
        let glow = material(&info("eut2.dif.lum.add", BlendMode::Additive), None);
        assert_eq!(glow.alpha_mode, AlphaMode::Add);
    }

    #[test]
    fn chrome_becomes_bright_metal_and_plain_materials_stay_dull() {
        // A typical chrome (`research/evidence/material-survey.txt`).
        let chrome_info = MaterialInfo {
            path: String::new(),
            effect: "eut2.dif.spec.weight.add.env.rfx".to_owned(),
            blend: BlendMode::Opaque,
            diffuse: [0.214041; 3],
            base_texture: None,
            paintable: false,
            emissive: false,
            decal: false,
            specular: Some([1.0; 3]),
            shininess: Some(70.0),
            env_factor: Some([0.977402; 3]),
            fresnel: Some([1.0, 1.0]),
            reflection_texture: None,
        };
        let chrome = material(&chrome_info, None);
        assert!((chrome.metallic - 0.977).abs() < 1e-3);
        assert!(chrome.base_color.to_linear().red > 0.9);
        assert!((chrome.perceptual_roughness - 0.408).abs() < 1e-3);

        let plain = material(
            &MaterialInfo {
                env_factor: None,
                ..chrome_info.clone()
            },
            None,
        );
        assert_eq!((plain.metallic, plain.perceptual_roughness), (0.1, 0.6));
        assert!((plain.base_color.to_linear().red - 0.214041).abs() < 1e-6);

        let glass = material(
            &MaterialInfo {
                effect: "eut2.glass.rfx".to_owned(),
                blend: BlendMode::Blend,
                ..chrome_info
            },
            None,
        );
        assert_eq!(glass.metallic, 0.1);
    }
}
