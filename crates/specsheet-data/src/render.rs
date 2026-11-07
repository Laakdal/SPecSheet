//! Renderer-independent mesh, material and texture data built from original
//! game files (see `docs/RENDERING.md`).
//!
//! Conventions applied here, once, so renderers need no SCS knowledge:
//! * Triangle winding is flipped to counter-clockwise front faces (game data
//!   is clockwise: 99.3 % of sampled triangles, `research/evidence/geometry-survey.txt`).
//! * Only parts visible in the requested variant are emitted.
//! * Texture surfaces are unpacked; BGRA is swizzled to RGBA.

use specsheet_archive::{
    pixel_layout, AddressMode, ArchiveError, ImageMeta, PackedTexture, PixelLayout, SampleMeta,
};
use specsheet_formats::mat::{MatValue, Material};
use specsheet_formats::pmg::Locator;
use specsheet_formats::sii::resolve_relative;
use specsheet_formats::{dds, tobj};

use crate::error::{GameDataError, Result};
use crate::fs::GameFs;
use crate::model::Model;

/// How a material is drawn, derived from its effect name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendMode {
    /// Drawn opaque.
    Opaque,
    /// Alpha-tested cut-out (`.a` effects).
    AlphaTest,
    /// Alpha-blended (glass, decals, overlays, water).
    Blend,
    /// Additively blended (`add` not followed by `env`, e.g. light glows).
    Additive,
    /// Never drawn in colour; only casts sun shadows (`shadowonly`,
    /// shadows-and-fake-lights.md §1).
    ShadowOnly,
    /// Not drawn in a preview (helper effects).
    Hidden,
}

/// What the renderer needs from one `.mat`.
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialInfo {
    /// The `.mat` path.
    pub path: String,
    /// The effect name, e.g. `eut2.dif.spec.add.env.rfx`.
    pub effect: String,
    /// How it is drawn.
    pub blend: BlendMode,
    /// The `.mat` `diffuse` colour.
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

/// Placeholder textures of the cockpit's screens, which the engine draws
/// its GPS and dashboard displays into (Hypothesis: the shipped textures
/// only say "GPS"). With the engine off, as on the tuning screen, the
/// screens are dark (the user's report).
const ENGINE_SCREENS: [&str; 2] = [
    "/vehicle/truck/share/gps.tobj",
    "/vehicle/truck/share/dashboard.tobj",
];

impl MaterialInfo {
    /// Draws one of the engine's screens ([`ENGINE_SCREENS`]).
    pub fn is_engine_screen(&self) -> bool {
        self.base_texture
            .as_deref()
            .is_some_and(|t| ENGINE_SCREENS.contains(&t))
    }

    /// A vehicle light's glow (`eut2.flare`, flare.md).
    pub fn is_flare(&self) -> bool {
        self.effect.split('.').any(|t| t == "flare")
    }

    /// Casts sun shadows: `shadowonly`, and the `shadow` flavour, which adds
    /// a shadow-map pass (dif-spec.md §2). Other effects cast none.
    pub fn casts_shadow(&self) -> bool {
        self.blend == BlendMode::ShadowOnly || self.effect.split('.').any(|t| t == "shadow")
    }

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
        let blend = if has("shadowonly") {
            BlendMode::ShadowOnly
        } else if has("flare") {
            // `eut2.flare` blends additively (flare.md §2).
            BlendMode::Additive
        } else if ["fakeshadow", "decalshadow", "mlaaweight"]
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
    /// Index into [`MeshData::materials`].
    pub material: usize,
    /// Vertex positions, model space.
    pub positions: Vec<[f32; 3]>,
    /// Vertex normals.
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
    /// Triangle-list indices, counter-clockwise front faces.
    pub indices: Vec<u32>,
}

/// One look and variant of a model, ready for upload ([`build_mesh`]).
#[derive(Clone, Debug, PartialEq)]
pub struct MeshData {
    /// The `.pmd` path.
    pub path: String,
    /// The look's materials.
    pub materials: Vec<MaterialInfo>,
    /// The pieces of the chosen variant and parts.
    pub pieces: Vec<MeshPiece>,
    /// Bounds, smallest corner.
    pub aabb_min: [f32; 3],
    /// Bounds, largest corner.
    pub aabb_max: [f32; 3],
    /// The visible parts' light locators: where vehicle lights sit (F19).
    pub lights: Vec<LightLocator>,
}

/// A locator whose hookup names a vehicle light (`flare.*`, F19).
#[derive(Clone, Debug, PartialEq)]
pub struct LightLocator {
    /// The light unit's name, e.g. `flare.vehicle.headl`.
    pub hookup: String,
    /// Position in model space.
    pub position: [f32; 3],
    /// Rotation quaternion in `(w, x, y, z)` order.
    pub rotation_wxyz: [f32; 4],
}

impl LightLocator {
    /// The locator as a light, when its hookup names one.
    pub fn of(locator: &Locator) -> Option<Self> {
        let hookup = locator
            .hookup
            .as_ref()
            .filter(|h| h.starts_with("flare."))?;
        Some(Self {
            hookup: hookup.clone(),
            position: locator.position,
            rotation_wxyz: locator.rotation_wxyz,
        })
    }
}

/// The UV slot of the "UV Normal" stream (`docs/formats/models.md`, piece +8).
const NORMAL_UV_SLOT: usize = 6;
/// The UV slot `texture_mult` samples.
const MULT_UV_SLOT: usize = 1;
/// The UV slots truck paint reads its mask through, without and with
/// `alternate_uvset` (truckpaint.md §2).
const PAINT_UV_SLOT: usize = 1;
const ALTERNATE_PAINT_UV_SLOT: usize = 2;

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

/// The UV sets truck paint samples on a piece: the mask on the set routed to
/// UV slot 1, and with `alternate_uvset` the set routed to slot 2 (the paint
/// shader reads its mask through those slots, truckpaint.md §2; the routing
/// is the piece's +8 field, `research/evidence/pmg-uv-routing.txt`). A slot
/// the piece doesn't route falls back to [`paint_uv_set`].
pub fn paint_uv_sets(
    slot1: Option<usize>,
    slot2: Option<usize>,
    sets: usize,
) -> (Option<usize>, Option<usize>) {
    (
        slot1.or_else(|| paint_uv_set(sets, false)),
        slot2.or_else(|| paint_uv_set(sets, true)),
    )
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
    let mut lights = Vec::new();
    for (part_index, part) in model.pmg.parts.iter().enumerate() {
        let selected = parts.is_empty() || parts.contains(&part.name);
        if !selected || !model.pmd.is_part_visible(variant, part_index) {
            continue;
        }
        lights.extend(
            model
                .pmg
                .locators
                .get(part.locators.clone())
                .unwrap_or_default()
                .iter()
                .filter_map(LightLocator::of),
        );
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
            let (paint_set, alternate_set) = if painted {
                paint_uv_sets(
                    piece.uv_set_for_slot(PAINT_UV_SLOT),
                    piece.uv_set_for_slot(ALTERNATE_PAINT_UV_SLOT),
                    sets,
                )
            } else {
                (None, None)
            };
            let alternate_set = alternate_set.filter(|&set| Some(set) != paint_set);
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
        lights,
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
    /// BC1 (DXT1).
    Bc1,
    /// BC2 (DXT3).
    Bc2,
    /// BC3 (DXT5).
    Bc3,
    /// BC4, one channel.
    Bc4,
    /// BC5, two channels (normal maps).
    Bc5,
    /// BC7 (mod textures only).
    Bc7,
    /// Uncompressed RGBA, 8 bits per channel.
    Rgba8,
    /// Uncompressed, one 8-bit channel.
    R8,
    /// R9G9B9E5_SHAREDEXP: linear HDR, three 9-bit mantissas sharing a
    /// 5-bit exponent (skyboxes, HDR UI images).
    Rgb9e5,
    /// Linear RGBA half floats, from a mod's 32-bit float texture (wgpu
    /// filters 32-bit floats only with an optional feature).
    Rgba16Float,
}

/// One mip level.
#[derive(Clone, Debug, PartialEq)]
pub struct MipLevel {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// The pixels, tightly packed.
    pub data: Vec<u8>,
}

/// A 2D texture ready for upload: tightly packed mip chain of face 0.
#[derive(Clone, Debug, PartialEq)]
pub struct TextureData {
    /// Upload format.
    pub format: TextureFormat,
    /// Colour data in sRGB (else linear).
    pub srgb: bool,
    /// The mip chain, largest first.
    pub mips: Vec<MipLevel>,
    /// Wrap along u (else clamp).
    pub repeat_u: bool,
    /// Wrap along v (else clamp).
    pub repeat_v: bool,
}

/// A cube texture ready for upload: six faces in their stored order, each a
/// tightly packed mip chain. The order is taken to be +x, −x, +y, −y, +z, −z
/// (the DDS convention; y Confirmed, see `docs/formats/textures.md`). The
/// game's lookup mirrors world z (F18).
#[derive(Clone, Debug, PartialEq)]
pub struct CubeTextureData {
    /// Upload format.
    pub format: TextureFormat,
    /// Colour data in sRGB (else linear).
    pub srgb: bool,
    /// Six faces, each a mip chain, largest first.
    pub faces: Vec<Vec<MipLevel>>,
}

/// Loads `.tobj` texture `path` and converts it for upload.
pub fn load_texture(fs: &GameFs, path: &str) -> Result<TextureData> {
    convert_texture(path, &read_texture(fs, path)?)
}

/// Loads the six-face cube texture `path` (e.g. a material's
/// `texture_reflection`) and converts it for upload.
pub fn load_cube_texture(fs: &GameFs, path: &str) -> Result<CubeTextureData> {
    convert_cube(path, &read_texture(fs, path)?)
}

/// The texture `path` as the game packs it, or, when the winning `.tobj` is
/// a loose file of a mod package (D20), built from that `.tobj` and the
/// `.dds` it names, read through the file system so mod ranking applies.
fn read_texture(fs: &GameFs, path: &str) -> Result<PackedTexture> {
    match fs.vfs().read_texture(path.trim_start_matches('/')) {
        Err(ArchiveError::NotPackedTexture(_)) => loose_texture(fs, path),
        other => Ok(other?),
    }
}

/// A loose `.tobj` and its `.dds` (`research/evidence/tobj-loose.txt`):
/// the header gives the address modes, whether mips are used and, for a
/// `.dds` that names no colour space, whether the data is linear; a cube
/// names six `.dds`, one per face.
fn loose_texture(fs: &GameFs, path: &str) -> Result<PackedTexture> {
    let invalid = |message: String| GameDataError::Definition {
        path: path.to_owned(),
        message,
    };
    let header = tobj::parse(&fs.read(path)?)
        .ok_or_else(|| invalid("not a texture object (.tobj)".to_owned()))?;
    let mut faces = Vec::with_capacity(header.paths.len());
    for texture in &header.paths {
        let texture = resolve_relative(path, texture);
        let bytes = fs.read(&texture)?;
        faces.push(dds::parse(&bytes).map_err(|e| invalid(format!("{texture}: {e}")))?);
    }
    let first = faces
        .first()
        .ok_or_else(|| invalid("names no texture".to_owned()))?;
    let (width, height, format, mips) = (
        first.width,
        first.height,
        first.dxgi_format,
        first.mip_count,
    );
    if faces
        .iter()
        .any(|f| (f.width, f.height, f.dxgi_format, f.mip_count) != (width, height, format, mips))
    {
        return Err(invalid("cube faces differ in size or format".to_owned()));
    }
    let dxgi_format = if first.colour_space_known || header.linear {
        format
    } else {
        dds::srgb_variant(format)
    };
    let face_count: u32 = faces.iter().map(|f| f.face_count).sum();
    let image = ImageMeta {
        width,
        height,
        mip_count: mips,
        dxgi_format,
        is_cube: face_count == 6,
        face_count,
        pitch_alignment: 1,
        image_alignment: 1,
    };
    let address = |code: u8| AddressMode::from_bits(u32::from(code));
    let sample = SampleMeta {
        mag_linear: true,
        min_linear: true,
        address_u: address(header.address[0]),
        address_v: address(header.address[1]),
    };
    let mut texture = PackedTexture {
        image,
        sample,
        surfaces: faces.into_iter().flat_map(|f| f.data).collect(),
    };
    if !header.mipmaps && mips > 1 {
        // The game uses mip 0 only; keep it and drop the rest per face.
        texture = mip_zero(texture).map_err(invalid)?;
    }
    Ok(texture)
}

/// `texture` with only its largest mip on each face.
fn mip_zero(texture: PackedTexture) -> std::result::Result<PackedTexture, String> {
    let surfaces: Vec<u8> = texture
        .unpack()?
        .into_iter()
        .filter(|s| s.mip == 0)
        .flat_map(|s| s.data)
        .collect();
    Ok(PackedTexture {
        image: ImageMeta {
            mip_count: 1,
            ..texture.image
        },
        surfaces,
        ..texture
    })
}

pub(crate) fn convert_cube(path: &str, packed: &PackedTexture) -> Result<CubeTextureData> {
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

pub(crate) fn convert_texture(path: &str, packed: &PackedTexture) -> Result<TextureData> {
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
        97 | 98 => (TextureFormat::Bc7, false, false),
        99 => (TextureFormat::Bc7, true, false),
        27 | 28 => (TextureFormat::Rgba8, false, false),
        29 => (TextureFormat::Rgba8, true, false),
        87 | 88 => (TextureFormat::Rgba8, false, true),
        90..=93 => (TextureFormat::Rgba8, dxgi == 91 || dxgi == 93, true),
        60..=64 => (TextureFormat::R8, false, false),
        67 => (TextureFormat::Rgb9e5, false, false),
        // R32G32B32A32 typeless and float; the integer variants are not.
        1 | 2 => (TextureFormat::Rgba16Float, false, false),
        other => return Err(format_error(format!("unsupported DXGI format {other}"))),
    };
    let to_half = format == TextureFormat::Rgba16Float;
    let x_channel_is_opaque = matches!(dxgi, 88 | 92 | 93);
    if pixel_layout(dxgi).is_none() {
        return Err(format_error(format!("no layout for DXGI format {dxgi}")));
    }
    let mut faces = vec![Vec::new(); packed.image.face_count as usize];
    for surface in packed.unpack().map_err(format_error)? {
        let mut data = surface.data;
        if to_half {
            data = data
                .chunks_exact(4)
                .flat_map(|v| {
                    f32_to_f16(f32::from_le_bytes([v[0], v[1], v[2], v[3]])).to_le_bytes()
                })
                .collect();
        }
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

/// The IEEE half float nearest to `value` (ties to even); NaN stays NaN,
/// and values past the largest half become infinite.
fn f32_to_f16(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exponent = ((bits >> 23) & 0xff) as i32;
    let mantissa = bits & 0x7f_ffff;
    if exponent == 0xff {
        let nan = if mantissa != 0 { 0x0200 } else { 0 };
        return sign | 0x7c00 | nan;
    }
    // The exponent rebiased from 127 to 15.
    let half_exponent = exponent - 112;
    if half_exponent >= 0x1f {
        return sign | 0x7c00;
    }
    // Rounds `m >> shift` to the nearest integer, ties to even.
    let round = |m: u32, shift: u32| {
        let kept = m >> shift;
        let rest = m & ((1 << shift) - 1);
        let half = 1 << (shift - 1);
        if rest > half || (rest == half && kept & 1 == 1) {
            kept + 1
        } else {
            kept
        }
    };
    if half_exponent <= 0 {
        // Subnormal half: the value in units of 2^-24.
        if half_exponent < -10 {
            return sign;
        }
        let shift = (14 - half_exponent) as u32;
        return sign | round(mantissa | 0x80_0000, shift) as u16;
    }
    // A carry out of the mantissa raises the exponent, up to infinity.
    let normal = (half_exponent as u32) << 23 | mantissa;
    sign | round(normal, 13) as u16
}

/// Bytes per block or pixel of an upload format, for consumers that
/// validate sizes.
pub fn texture_block_bytes(format: TextureFormat) -> (u32, PixelLayout) {
    match format {
        TextureFormat::Bc1 | TextureFormat::Bc4 => (8, PixelLayout::Block { bytes: 8 }),
        TextureFormat::Bc2 | TextureFormat::Bc3 | TextureFormat::Bc5 | TextureFormat::Bc7 => {
            (16, PixelLayout::Block { bytes: 16 })
        }
        TextureFormat::Rgba8 | TextureFormat::Rgb9e5 => {
            (4, PixelLayout::Linear { bytes_per_pixel: 4 })
        }
        TextureFormat::R8 => (1, PixelLayout::Linear { bytes_per_pixel: 1 }),
        TextureFormat::Rgba16Float => (8, PixelLayout::Linear { bytes_per_pixel: 8 }),
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
    fn paint_follows_the_pieces_uv_routing() {
        // 0xf0fff210 (the Actros 2014 Gigaspace cab): slot 1 = set 1,
        // slot 2 = set 2, as the fixed rule.
        assert_eq!(paint_uv_sets(Some(1), Some(2), 3), (Some(1), Some(2)));
        // 0xf2fff110 (e.g. the DAF 2021 UK door): slot 2 reads set 1, where
        // the fixed rule took set 2, the normal-map set.
        assert_eq!(paint_uv_sets(Some(1), Some(1), 3), (Some(1), Some(1)));
        // Unrouted slots fall back to the fixed rule.
        assert_eq!(paint_uv_sets(None, None, 3), (Some(1), Some(2)));
    }

    #[test]
    fn vertex_colours_decode_lowest_byte_first() {
        assert_eq!(vertex_colors(&[]), None);
        let colors = vertex_colors(&[0xff00_80ff]).expect("colours");
        assert_eq!(colors, [[1.0, 128.0 / 255.0, 0.0, 1.0]]);
    }

    #[test]
    fn flares_blend_additively_and_only_flare_locators_are_lights() {
        let flare = material("eut2.flare.rfx");
        assert!(flare.is_flare());
        assert_eq!(flare.blend, BlendMode::Additive);
        assert!(!material("eut2.dif.spec.rfx").is_flare());
        let locator = |hookup: Option<&str>| Locator {
            name: "light_h_0".to_owned(),
            position: [1.0, 2.0, 3.0],
            scale: 1.0,
            rotation_wxyz: [1.0, 0.0, 0.0, 0.0],
            hookup: hookup.map(str::to_owned),
        };
        let light = LightLocator::of(&locator(Some("flare.vehicle.headl"))).expect("a light");
        assert_eq!(light.hookup, "flare.vehicle.headl");
        assert_eq!(light.position, [1.0, 2.0, 3.0]);
        assert_eq!(LightLocator::of(&locator(Some("license.plate"))), None);
        assert_eq!(LightLocator::of(&locator(None)), None);
    }

    #[test]
    fn only_shadowonly_and_the_shadow_flavour_cast_shadows() {
        assert!(material("eut2.shadowonly.rfx").casts_shadow());
        assert!(material("eut2.shadowonly.nocull.rfx").casts_shadow());
        assert!(material("eut2.dif.spec.add.env.nofresnel.shadow.rfx").casts_shadow());
        assert!(!material("eut2.dif.spec.add.env.rfx").casts_shadow());
        assert!(!material("eut2.fakeshadow.rfx").casts_shadow());
        assert!(!material("eut2.decalshadow.rfx").casts_shadow());
    }

    #[test]
    fn classifies_effects() {
        assert_eq!(material("eut2.shadowonly.rfx").blend, BlendMode::ShadowOnly);
        assert_eq!(material("eut2.fakeshadow.rfx").blend, BlendMode::Hidden);
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

    /// A loose `.tobj` (header bytes from `tobj-loose.txt`) naming `dds`.
    fn loose_tobj(clamp: bool, linear: bool, dds: &str) -> Vec<u8> {
        let mut bytes = vec![0u8; 0x28];
        bytes[..4].copy_from_slice(&tobj::MAGIC);
        bytes[0x18] = 2;
        bytes[0x1c] = 3;
        if clamp {
            bytes[0x1e] = 2;
            bytes[0x1f] = 2;
        }
        bytes[0x26] = u8::from(linear);
        bytes.extend((dds.len() as u64).to_le_bytes());
        bytes.extend(dds.as_bytes());
        bytes
    }

    /// A legacy DXT5 `.dds` of 4x4 pixels, one mip.
    fn dxt5() -> Vec<u8> {
        let mut b = vec![0u8; 128];
        b[..4].copy_from_slice(b"DDS ");
        b[4..8].copy_from_slice(&124u32.to_le_bytes());
        b[12..16].copy_from_slice(&4u32.to_le_bytes());
        b[16..20].copy_from_slice(&4u32.to_le_bytes());
        b[80..84].copy_from_slice(&4u32.to_le_bytes()); // FourCC flag
        b[84..88].copy_from_slice(b"DXT5");
        b.extend([0x55u8; 16]);
        b
    }

    /// D20: a mod's loose texture loads through the file system; a `.dds`
    /// without a colour space is sRGB unless the `.tobj` marks it linear.
    #[test]
    fn loads_a_mods_loose_tobj_and_dds() {
        use specsheet_archive::{Device, Vfs, ZipArchive, ZipWriter};
        let mut writer = ZipWriter::new();
        writer
            .add(
                "vehicle/a.tobj",
                &loose_tobj(false, false, "/vehicle/a.dds"),
            )
            .unwrap();
        writer
            .add("vehicle/b.tobj", &loose_tobj(true, true, "b.dds"))
            .unwrap();
        writer.add("vehicle/a.dds", &dxt5()).unwrap();
        writer.add("vehicle/b.dds", &dxt5()).unwrap();
        let file = std::env::temp_dir().join(format!("specsheet-loose-{}.zip", std::process::id()));
        std::fs::write(&file, writer.finish().unwrap()).unwrap();
        let mut vfs = Vfs::new();
        vfs.mount_device(Device::Zip(ZipArchive::open(&file).unwrap()), "");
        let fs = GameFs::new(vfs);

        let colour = load_texture(&fs, "/vehicle/a.tobj").expect("a");
        assert_eq!(colour.format, TextureFormat::Bc3);
        assert!(colour.srgb && colour.repeat_u && colour.repeat_v);
        assert_eq!(colour.mips.len(), 1);
        assert_eq!(colour.mips[0].data, [0x55; 16]);
        // A relative path, clamped and linear.
        let data = load_texture(&fs, "/vehicle/b.tobj").expect("b");
        assert!(!data.srgb && !data.repeat_u && !data.repeat_v);
        drop(fs);
        let _ = std::fs::remove_file(file);
    }

    /// A legacy 32-bit float `.dds` (FourCC 0x74) of one pixel.
    fn float_dds(pixel: [f32; 4]) -> Vec<u8> {
        let mut b = dxt5();
        b.truncate(128);
        b[84..88].copy_from_slice(&[0x74, 0, 0, 0]);
        b.extend(pixel.iter().flat_map(|v| v.to_le_bytes()));
        for (at, v) in [(12, 1u32), (16, 1)] {
            b[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
        b
    }

    /// D20: a mod's 32-bit float texture uploads as half floats, which
    /// every GPU can filter; float data is linear whatever the `.tobj` says.
    #[test]
    fn loads_a_float_dds_as_half_floats() {
        use specsheet_archive::{Device, Vfs, ZipArchive, ZipWriter};
        let mut writer = ZipWriter::new();
        writer
            .add("sky/f.tobj", &loose_tobj(false, false, "f.dds"))
            .unwrap();
        writer
            .add("sky/f.dds", &float_dds([1.0, 0.5, -2.0, 65504.0]))
            .unwrap();
        let file = std::env::temp_dir().join(format!("specsheet-float-{}.zip", std::process::id()));
        std::fs::write(&file, writer.finish().unwrap()).unwrap();
        let mut vfs = Vfs::new();
        vfs.mount_device(Device::Zip(ZipArchive::open(&file).unwrap()), "");
        let fs = GameFs::new(vfs);
        let texture = load_texture(&fs, "/sky/f.tobj").expect("float");
        assert_eq!(
            (texture.format, texture.srgb),
            (TextureFormat::Rgba16Float, false)
        );
        let halves: Vec<u8> = [0x3c00u16, 0x3800, 0xc000, 0x7bff]
            .iter()
            .flat_map(|h| h.to_le_bytes())
            .collect();
        assert_eq!(texture.mips[0].data, halves);
        drop(fs);
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn rounds_floats_to_the_nearest_half() {
        let cases: [(f32, u16); 12] = [
            (0.0, 0x0000),
            (-0.0, 0x8000),
            (1.0, 0x3c00),
            (1.0 + 1.0 / 2048.0, 0x3c00), // halfway, ties to even
            (1.0 + 3.0 / 2048.0, 0x3c02), // halfway, ties to even (up)
            (65504.0, 0x7bff),
            (65520.0, 0x7c00), // rounds past the largest half
            (1.0e9, 0x7c00),
            (f32::NEG_INFINITY, 0xfc00),
            (2.0f32.powi(-24), 0x0001), // smallest subnormal
            (2.0f32.powi(-14), 0x0400), // smallest normal
            (2.0f32.powi(-26), 0x0000), // underflows to zero
        ];
        for (value, half) in cases {
            assert_eq!(f32_to_f16(value), half, "{value}");
        }
        assert_eq!(f32_to_f16(f32::NAN) & 0x7c00, 0x7c00);
        assert_ne!(f32_to_f16(f32::NAN) & 0x03ff, 0);
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
