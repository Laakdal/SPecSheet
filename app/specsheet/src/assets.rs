//! Converts `game-data` meshes, materials and textures into Bevy assets.
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
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat as GpuFormat};
use game_data::render::{
    build_mesh, load_texture, texture_block_bytes, BlendMode, MaterialInfo, MeshData, MeshPiece,
    TextureData, TextureFormat,
};
use game_data::session::GameSession;

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
    /// Materials painted with the truck's paint colour.
    pub paintable: Vec<Handle<StandardMaterial>>,
    pub aabb_min: Vec3,
    pub aabb_max: Vec3,
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
    let paintable = mesh
        .materials
        .iter()
        .zip(&materials)
        .filter(|(info, _)| info.paintable)
        .map(|(_, handle)| handle.clone())
        .collect();
    let mut pieces = Vec::new();
    for (index, piece) in mesh.pieces.into_iter().enumerate() {
        let Some(material) = materials.get(piece.material) else {
            warnings.push(format!("{}: piece {index} has no material", mesh.path));
            continue;
        };
        match piece_mesh(piece) {
            Ok(m) => pieces.push((stores.meshes.add(m), material.clone())),
            Err(error) => warnings.push(format!("{}: piece {index}: {error}", mesh.path)),
        }
    }
    let model = GpuModel {
        pieces,
        paintable,
        aabb_min: Vec3::from(mesh.aabb_min),
        aabb_max: Vec3::from(mesh.aabb_max),
    };
    (Arc::new(model), warnings)
}

fn piece_mesh(piece: MeshPiece) -> Result<Mesh, String> {
    let count = piece.positions.len();
    if piece.normals.len() != count {
        return Err("normal count differs from position count".to_owned());
    }
    if piece.uvs.as_ref().is_some_and(|uvs| uvs.len() != count) {
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

/// The PBR stand-in for a game material. This is an approximation, not the
/// game's shaders.
pub fn material(info: &MaterialInfo, texture: Option<Handle<Image>>) -> StandardMaterial {
    let [r, g, b] = info.diffuse;
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
        metallic: if info.paintable { 0.35 } else { 0.1 },
        perceptual_roughness: if info.paintable { 0.35 } else { 0.6 },
        alpha_mode,
        depth_bias: if info.decal { DECAL_DEPTH_BIAS } else { 0.0 },
        ..default()
    }
}

/// Largest texture side accepted (the wgpu limit on desktop adapters).
const MAX_TEXTURE_SIZE: u32 = 16384;

/// Builds a GPU image from a converted game texture, checking every size
/// wgpu would otherwise panic on.
pub fn texture_image(data: TextureData, bc_supported: bool) -> Result<Image, String> {
    let block = matches!(
        data.format,
        TextureFormat::Bc1
            | TextureFormat::Bc2
            | TextureFormat::Bc3
            | TextureFormat::Bc4
            | TextureFormat::Bc5
    );
    if block && !bc_supported {
        return Err("GPU lacks BC texture compression".to_owned());
    }
    let format = gpu_format(data.format, data.srgb);
    let (unit_bytes, _) = texture_block_bytes(data.format);
    let base = data.mips.first().ok_or("texture has no surfaces")?;
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
    if data.mips.len() > max_mips as usize {
        return Err(format!(
            "{} mips exceed the chain of {max_mips}",
            data.mips.len()
        ));
    }
    let mut bytes = Vec::new();
    for (level, mip) in data.mips.iter().enumerate() {
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
    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        bytes,
        format,
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
    use game_data::render::MipLevel;

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
            indices,
        };
        assert!(piece_mesh(piece(vec![0, 1, 2], 3)).is_ok());
        assert!(piece_mesh(piece(vec![0, 1, 3], 3)).is_err());
        assert!(piece_mesh(piece(vec![0, 1], 3)).is_err());
        assert!(piece_mesh(piece(vec![0, 1, 2], 2)).is_err());
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
        };
        let glass = material(&info("eut2.glass", BlendMode::Blend), None);
        assert_eq!(glass.alpha_mode, AlphaMode::Blend);
        assert_eq!(glass.base_color.alpha(), GLASS_OPACITY);
        let cutout = material(&info("eut2.dif.a", BlendMode::AlphaTest), None);
        assert_eq!(cutout.alpha_mode, AlphaMode::Mask(0.5));
        let glow = material(&info("eut2.dif.lum.add", BlendMode::Additive), None);
        assert_eq!(glow.alpha_mode, AlphaMode::Add);
    }
}
