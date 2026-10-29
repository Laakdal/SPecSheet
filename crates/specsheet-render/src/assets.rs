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
    Extent3d, Face, TextureDimension, TextureFormat as GpuFormat, TextureViewDescriptor,
    TextureViewDimension,
};
use specsheet_core::colour::srgb_to_linear;
use specsheet_data::render::{
    build_mesh, load_texture, texture_block_bytes, BlendMode, CubeTextureData, LightLocator,
    MaterialInfo, MeshData, MeshPiece, MipLevel, TextureData, TextureFormat,
};
use specsheet_session::GameSession;

use crate::{DifSpecSurface, PaintSurface};

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
    let surfaces: Vec<DifSpecSurface> = mesh
        .materials
        .iter()
        .filter_map(DifSpecSurface::of)
        .collect();
    let mut paths: Vec<&str> = mesh
        .materials
        .iter()
        .filter_map(|m| m.base_texture.as_deref())
        .chain(surfaces.iter().flat_map(DifSpecSurface::texture_paths))
        .collect();
    paths.sort();
    paths.dedup();
    let textures = paths
        .into_iter()
        .map(|path| {
            let texture = load_texture(game.fs(), path).map_err(|e| e.to_string());
            (path.to_owned(), texture)
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
    /// Each piece's mesh and its `.mat`'s `StandardMaterial` stand-in.
    pub pieces: Vec<(Handle<Mesh>, Handle<StandardMaterial>)>,
    /// Per piece: the clean-room material it takes instead of the stand-in.
    pub game: Vec<Option<GamePiece>>,
    /// Per piece: whether it casts sun shadows
    /// ([`MaterialInfo::casts_shadow`]).
    pub casts_shadow: Vec<bool>,
    /// The visible parts' light locators (F19).
    pub lights: Vec<LightLocator>,
    /// The reflection cube (`texture_reflection`) of every reflective
    /// material, once per material; the viewer uses the most common one as
    /// the environment map that lights the stand-ins.
    pub reflections: Vec<String>,
    pub aabb_min: Vec3,
    pub aabb_max: Vec3,
}

/// A piece drawn by a clean-room game material (`crate::render`).
pub enum GamePiece {
    /// Truck paint, which takes the paint of the placement it belongs to.
    Paint(PaintPiece),
    DifSpec(DifSpecSurface),
    /// `shadowonly`: only casts sun shadows (`crate::ShadowOnlyMaterial`).
    ShadowOnly,
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
    let mut game = Vec::new();
    let mut casts_shadow = Vec::new();
    for (index, piece) in mesh.pieces.into_iter().enumerate() {
        let (Some(material), Some(info)) = (
            materials.get(piece.material),
            mesh.materials.get(piece.material),
        ) else {
            warnings.push(format!("{}: piece {index} has no material", mesh.path));
            continue;
        };
        let paint = PaintSurface::of(info);
        let dif_spec = DifSpecSurface::of(info).map(|mut surface| {
            surface.resolve_textures(|path| stores.textures.get(path).cloned().flatten());
            if !surface.route_uvs(&piece) {
                warnings.push(format!(
                    "{}: piece {index}: its mult and normal-map UV sets differ from UV0 \
                     and from each other; drawn without the normal map",
                    mesh.path
                ));
            }
            surface
        });
        let attributes = MeshAttributes {
            paint_uv: paint.is_some(),
            // flare.md §3 multiplies by 2·c, as the game materials do.
            colors: paint.is_some() || dif_spec.is_some() || info.is_flare(),
            tangents: dif_spec
                .as_ref()
                .is_some_and(DifSpecSurface::needs_tangents),
            normal_uv: dif_spec
                .as_ref()
                .is_some_and(DifSpecSurface::needs_normal_uvs),
            mult_uv: dif_spec
                .as_ref()
                .is_some_and(DifSpecSurface::needs_mult_uvs),
        };
        let alternate = match (&paint, &piece.alternate_paint_uvs) {
            (Some(_), Some(uvs)) => Some(MeshPiece {
                paint_uvs: Some(uvs.clone()),
                ..piece.clone()
            }),
            _ => None,
        };
        match piece_mesh(piece, attributes).and_then(|m| {
            let alternate = alternate.map(|p| piece_mesh(p, attributes)).transpose()?;
            Ok((m, alternate))
        }) {
            Ok((m, alternate)) => {
                pieces.push((stores.meshes.add(m), material.clone()));
                casts_shadow.push(info.casts_shadow());
                let alternate_mesh = alternate.map(|m| stores.meshes.add(m));
                game.push(match (paint, dif_spec) {
                    _ if info.blend == BlendMode::ShadowOnly => Some(GamePiece::ShadowOnly),
                    (Some(surface), _) => Some(GamePiece::Paint(PaintPiece {
                        surface,
                        alternate_mesh,
                    })),
                    (None, Some(surface)) => Some(GamePiece::DifSpec(surface)),
                    (None, None) => None,
                });
            }
            Err(error) => warnings.push(format!("{}: piece {index}: {error}", mesh.path)),
        }
    }
    let reflections = mesh
        .materials
        .iter()
        .filter(|info| {
            reflective_surface(info).is_some()
                || DifSpecSurface::of(info).is_some_and(|s| s.reflection().is_some())
        })
        .filter_map(|info| info.reflection_texture.clone())
        .collect();
    let model = GpuModel {
        pieces,
        game,
        casts_shadow,
        lights: mesh.lights,
        reflections,
        aabb_min: Vec3::from(mesh.aabb_min),
        aabb_max: Vec3::from(mesh.aabb_max),
    };
    (Arc::new(model), warnings)
}

/// Which optional attributes a piece's mesh carries.
#[derive(Clone, Copy, Debug, Default)]
struct MeshAttributes {
    /// The paint UV set as `UV_1` (truck paint).
    paint_uv: bool,
    /// The vertex colours, doubled: the game materials multiply by twice
    /// the colour, so 0.5 is neutral (dif-spec.md §3.2). The stand-ins do
    /// not use them.
    colors: bool,
    /// The stored tangents (`dif.spec` normal maps, dif-spec.md §3.2).
    tangents: bool,
    /// The "UV Normal" stream (UV slot 6) as `UV_1`, which the `tsnmapuv`
    /// flavours sample their normal map with (dif-spec.md §5.2).
    normal_uv: bool,
    /// The set routed to UV slot 1 as `UV_1`, which `texture_mult` samples
    /// (dif-spec-mult.md §2.1). It takes `UV_1` before the "UV Normal"
    /// stream; `DifSpecSurface::route_uvs` moves that stream to `UV_0` or
    /// onto this set. `UV_1` carries at most one of paint, mult and normal.
    mult_uv: bool,
}

/// The GPU mesh of `piece`.
fn piece_mesh(piece: MeshPiece, attributes: MeshAttributes) -> Result<Mesh, String> {
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
    // Only the streams this mesh carries are checked, so a bad stream the
    // piece's material does not use cannot drop the piece.
    let tangents = piece.tangents.filter(|_| attributes.tangents);
    if tangents.as_ref().is_some_and(|t| t.len() != count) {
        return Err("tangent count differs from position count".to_owned());
    }
    let normal_uvs = piece.normal_uvs.filter(|_| attributes.normal_uv);
    if normal_uvs.as_ref().is_some_and(|uvs| uvs.len() != count) {
        return Err("normal UV count differs from position count".to_owned());
    }
    let mult_uvs = piece.mult_uvs.filter(|_| attributes.mult_uv);
    if mult_uvs.as_ref().is_some_and(|uvs| uvs.len() != count) {
        return Err("mult UV count differs from position count".to_owned());
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
    if let Some(uvs) = piece
        .paint_uvs
        .filter(|_| attributes.paint_uv)
        .or(mult_uvs)
        .or(normal_uvs)
    {
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, uvs);
    }
    if let Some(tangents) = tangents {
        mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tangents);
    }
    if let Some(colors) = piece.colors.filter(|_| attributes.colors) {
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
/// Emissive strength of self-lit stand-ins in the game's scene units (ours):
/// the faint glow they had at Bevy's old exposure, 40 × 0.001. Lamps get
/// none: the tuning screen's lights are off (lamp.md §7).
const EMISSIVE_LUMINANCE: f32 = 0.04;
/// Depth bias that keeps decals in front of the surface they sit on.
const DECAL_DEPTH_BIAS: f32 = 8.0;
/// Below this perceptual roughness Bevy's specular highlights alias.
const MIN_ROUGHNESS: f32 = 0.089;

/// Base colour and metalness of a reflecting material drawn by its
/// stand-in (an `env` effect with an `env_factor` outside the `dif.spec`
/// family, e.g. `baked.spec.add.env`). The game adds the environment
/// reflection on top of a
/// dark diffuse; chrome is diffuse 0.21 with a factor of 0.98
/// (`research/evidence/material-survey.txt`). Here the factor's luminance
/// becomes metalness, and the base colour blends from the diffuse towards
/// the factor's colour by the same amount, so chrome reads as bright metal.
fn reflective_surface(info: &MaterialInfo) -> Option<([f32; 3], f32)> {
    if info.paintable
        || DifSpecSurface::of(info).is_some()
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
        BlendMode::Opaque | BlendMode::ShadowOnly | BlendMode::Hidden => AlphaMode::Opaque,
    };
    StandardMaterial {
        // `.mat` colours are linear, like paint colours.
        base_color: Color::linear_rgba(r, g, b, opacity),
        emissive: if info.emissive && !tokens.contains(&"lamp") {
            LinearRgba::rgb(EMISSIVE_LUMINANCE, EMISSIVE_LUMINANCE, EMISSIVE_LUMINANCE)
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
        // A flare is unlit, B·Kd·2c (flare.md §3; the viewer scales it by
        // the luminosity). Its quad is turned to the camera, so neither face
        // is culled.
        unlit: info.is_flare(),
        cull_mode: (!info.is_flare()).then_some(Face::Back),
        // A `.mat` without specular reflects nothing, not Bevy's default
        // 4 % (the chassis's `ui_shadow` quad turned into a grey sheet).
        reflectance: if info.specular == Some([0.0; 3]) {
            0.0
        } else {
            StandardMaterial::default().reflectance
        },
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
    // `Image::new` expects one mip level; the data holds the whole chain.
    let mut image = Image::new_uninit(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        gpu_format(data.format, data.srgb),
        RenderAssetUsages::RENDER_WORLD,
    );
    image.data = Some(bytes);
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
    let mut image = Image::new_uninit(
        Extent3d {
            width: side,
            height: side,
            depth_or_array_layers: 6,
        },
        TextureDimension::D2,
        gpu_format(data.format, data.srgb),
        RenderAssetUsages::RENDER_WORLD,
    );
    image.data = Some(bytes);
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
        (TextureFormat::Rgb9e5, _) => GpuFormat::Rgb9e5Ufloat,
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

    /// Bevy's `Image::new` asserts (in debug builds) that the data is one
    /// mip level; game textures carry the whole chain.
    #[test]
    fn uncompressed_textures_keep_their_mip_chain() {
        let rgba = TextureData {
            format: TextureFormat::Rgba8,
            mips: vec![
                MipLevel {
                    width: 4,
                    height: 4,
                    data: vec![0; 64],
                },
                MipLevel {
                    width: 2,
                    height: 2,
                    data: vec![0; 16],
                },
            ],
            ..bc1(4, 4, 1)
        };
        let image = texture_image(rgba.clone(), true).expect("valid texture");
        assert_eq!(image.texture_descriptor.mip_level_count, 2);
        assert_eq!(image.data.as_ref().map(Vec::len), Some(80));
        let cube = CubeTextureData {
            format: TextureFormat::Rgba8,
            srgb: true,
            faces: vec![rgba.mips; 6],
        };
        let image = cube_image(cube, true).expect("valid cube");
        assert_eq!(image.data.as_ref().map(Vec::len), Some(480));
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

    const ALL_ATTRIBUTES: MeshAttributes = MeshAttributes {
        paint_uv: true,
        colors: true,
        tangents: true,
        normal_uv: true,
        mult_uv: true,
    };

    fn triangle() -> MeshPiece {
        MeshPiece {
            material: 0,
            positions: vec![[0.0; 3]; 3],
            normals: vec![[0.0, 1.0, 0.0]; 3],
            uvs: Some(vec![[0.0; 2]; 3]),
            paint_uvs: None,
            alternate_paint_uvs: None,
            colors: None,
            normal_uvs: Some(vec![[0.5; 2]; 3]),
            mult_uvs: None,
            tangents: Some(vec![[1.0, 0.0, 0.0, -1.0]; 3]),
            indices: vec![0, 1, 2],
        }
    }

    #[test]
    fn normal_mapped_pieces_carry_tangents_and_the_normal_uvs() {
        let normal_mapped = MeshAttributes {
            colors: true,
            tangents: true,
            normal_uv: true,
            ..MeshAttributes::default()
        };
        let mesh = piece_mesh(triangle(), normal_mapped).expect("valid piece");
        assert!(mesh.contains_attribute(Mesh::ATTRIBUTE_TANGENT));
        assert!(mesh.contains_attribute(Mesh::ATTRIBUTE_UV_1));

        // Without a normal map the mesh is what it was before.
        let plain = piece_mesh(triangle(), MeshAttributes::default()).expect("valid piece");
        assert!(!plain.contains_attribute(Mesh::ATTRIBUTE_TANGENT));
        assert!(!plain.contains_attribute(Mesh::ATTRIBUTE_UV_1));

        // A short stream fails only when the mesh carries it.
        let short_tangents = MeshPiece {
            tangents: Some(vec![[1.0, 0.0, 0.0, 1.0]; 2]),
            ..triangle()
        };
        assert!(piece_mesh(short_tangents.clone(), normal_mapped).is_err());
        assert!(piece_mesh(short_tangents, MeshAttributes::default()).is_ok());
        let short_normal_uvs = MeshPiece {
            normal_uvs: Some(vec![[0.0; 2]; 4]),
            ..triangle()
        };
        assert!(piece_mesh(short_normal_uvs.clone(), normal_mapped).is_err());
        assert!(piece_mesh(short_normal_uvs, MeshAttributes::default()).is_ok());
    }

    /// The `UV_1` attribute of `mesh`, as its first vertex.
    fn uv_1(mesh: &Mesh) -> Option<[f32; 2]> {
        match mesh.attribute(Mesh::ATTRIBUTE_UV_1)? {
            bevy::mesh::VertexAttributeValues::Float32x2(uvs) => uvs.first().copied(),
            _ => None,
        }
    }

    #[test]
    fn mult_pieces_carry_their_mult_set_as_uv_1() {
        let mult = MeshPiece {
            mult_uvs: Some(vec![[0.25; 2]; 3]),
            ..triangle()
        };
        let mult_only = MeshAttributes {
            colors: true,
            mult_uv: true,
            ..MeshAttributes::default()
        };
        let mesh = piece_mesh(mult.clone(), mult_only).expect("valid piece");
        assert_eq!(uv_1(&mesh), Some([0.25; 2]));
        assert!(!mesh.contains_attribute(Mesh::ATTRIBUTE_TANGENT));

        // With a normal map that `route_uvs` moved onto UV0 or the mult set,
        // the mult set keeps UV_1.
        let both = MeshAttributes {
            tangents: true,
            normal_uv: true,
            ..mult_only
        };
        let mesh = piece_mesh(mult.clone(), both).expect("valid piece");
        assert_eq!(uv_1(&mesh), Some([0.25; 2]));
        assert!(mesh.contains_attribute(Mesh::ATTRIBUTE_TANGENT));

        // Without the mult layer the normal UVs are UV_1, as before.
        let mesh = piece_mesh(
            mult,
            MeshAttributes {
                mult_uv: false,
                ..both
            },
        )
        .expect("valid piece");
        assert_eq!(uv_1(&mesh), Some([0.5; 2]));

        let short = MeshPiece {
            mult_uvs: Some(vec![[0.0; 2]; 2]),
            ..triangle()
        };
        assert!(piece_mesh(short.clone(), mult_only).is_err());
        assert!(piece_mesh(short, MeshAttributes::default()).is_ok());
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
            normal_uvs: None,
            mult_uvs: None,
            tangents: None,
            indices,
        };
        assert!(piece_mesh(piece(vec![0, 1, 2], 3), MeshAttributes::default()).is_ok());
        assert!(piece_mesh(piece(vec![0, 1, 3], 3), MeshAttributes::default()).is_err());
        assert!(piece_mesh(piece(vec![0, 1], 3), MeshAttributes::default()).is_err());
        assert!(piece_mesh(piece(vec![0, 1, 2], 2), MeshAttributes::default()).is_err());
        let short_colours = MeshPiece {
            colors: Some(vec![[0.5; 4]; 2]),
            ..piece(vec![0, 1, 2], 3)
        };
        assert!(piece_mesh(short_colours, ALL_ATTRIBUTES).is_err());
        let short_paint_uvs = MeshPiece {
            paint_uvs: Some(vec![[0.0; 2]; 2]),
            ..piece(vec![0, 1, 2], 3)
        };
        assert!(piece_mesh(short_paint_uvs, ALL_ATTRIBUTES).is_err());
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
            additional_ambient: None,
            normal_texture: None,
            normal_detail_texture: None,
            mult_texture: None,
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
    fn a_material_without_specular_reflects_nothing() {
        // The chassis's `ui_shadow` quad: `eut2.dif.over`, specular 0, a
        // black texture whose alpha is the shadow. Bevy's default
        // reflectance made it a grey sheet at grazing angles.
        let info = |specular| MaterialInfo {
            path: String::new(),
            effect: "eut2.dif.over.rfx".to_owned(),
            blend: BlendMode::Blend,
            diffuse: [1.0; 3],
            base_texture: None,
            paintable: false,
            emissive: false,
            decal: true,
            specular,
            shininess: Some(1.0),
            env_factor: None,
            fresnel: None,
            reflection_texture: None,
            additional_ambient: None,
            normal_texture: None,
            normal_detail_texture: None,
            mult_texture: None,
        };
        assert_eq!(material(&info(Some([0.0; 3])), None).reflectance, 0.0);
        assert_eq!(
            material(&info(Some([0.5; 3])), None).reflectance,
            StandardMaterial::default().reflectance
        );
        assert_eq!(
            material(&info(None), None).reflectance,
            StandardMaterial::default().reflectance
        );
    }

    #[test]
    fn a_flare_is_an_unlit_additive_quad_seen_from_both_sides() {
        let info = MaterialInfo {
            path: String::new(),
            effect: "eut2.flare.rfx".to_owned(),
            blend: BlendMode::Additive,
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
            additional_ambient: None,
            normal_texture: None,
            normal_detail_texture: None,
            mult_texture: None,
        };
        let flare = material(&info, None);
        assert!(flare.unlit);
        assert_eq!(flare.alpha_mode, AlphaMode::Add);
        assert_eq!(flare.cull_mode, None);
        assert_eq!(flare.base_color, Color::linear_rgba(1.0, 1.0, 1.0, 1.0));
    }

    #[test]
    fn lamps_are_off_and_luminous_surfaces_glow_faintly_in_game_units() {
        let emissive = |effect: &str| {
            let info = MaterialInfo {
                path: String::new(),
                effect: effect.to_owned(),
                blend: BlendMode::Opaque,
                diffuse: [1.0; 3],
                base_texture: None,
                paintable: false,
                emissive: true,
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
            };
            material(&info, None).emissive
        };
        // The tuning screen's lights are off (lamp.md §7): no glow, so the
        // headlight is its plain surface, not a white blob.
        assert_eq!(emissive("eut2.lamp.add.env.rfx"), LinearRgba::BLACK);
        // Other self-lit surfaces keep the faint glow they had before the
        // lights moved to game units, far below white.
        let lum = emissive("eut2.dif.lum.rfx");
        assert!(lum.red > 0.0 && lum.red < 0.1, "{lum:?}");
    }

    #[test]
    fn chrome_becomes_bright_metal_and_plain_materials_stay_dull() {
        // A typical chrome (`research/evidence/material-survey.txt`).
        let chrome_info = MaterialInfo {
            path: String::new(),
            effect: "eut2.baked.spec.add.env.rfx".to_owned(),
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
            additional_ambient: None,
            normal_texture: None,
            normal_detail_texture: None,
            mult_texture: None,
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
                ..chrome_info.clone()
            },
            None,
        );
        assert_eq!(glass.metallic, 0.1);

        // The dif.spec family draws its own reflection; its stand-in keeps
        // the plain diffuse.
        let dif_spec = material(
            &MaterialInfo {
                effect: "eut2.dif.spec.weight.add.env.rfx".to_owned(),
                ..chrome_info
            },
            None,
        );
        assert_eq!(dif_spec.metallic, 0.1);
    }
}
