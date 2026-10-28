//! The `eut2.dif.spec` effect family as a Bevy material, from the spec
//! `docs/app/shader-specs/dif-spec.md`: diffuse with the base alpha as the
//! specular mask, unnormalised Blinn-Phong, and for `add.env` the `.mat`'s
//! own reflection cube with the linear fresnel, lit by the game's forward
//! model (`game_lighting.wgsl`). The tangent-space normal maps (`tsnmap`,
//! `tsnmapuv`, `tsnmapuv2`) perturb the normal; `oclu` is not drawn yet.
//!
//! The `dif.spec.mult.dif.spec` base is a flavour of this material, from
//! `docs/app/shader-specs/dif-spec-mult.md`: `texture_mult` on UV slot 1
//! multiplies the albedo, and its alpha the specular and reflection masks.
//! `dif.spec.decal.mult` is plain `dif.spec` drawn with a multiplicative
//! blend.

use std::collections::HashMap;

use bevy::asset::embedded_asset;
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::render::storage::ShaderBuffer;
use bevy::shader::ShaderRef;
use specsheet_data::render::{BlendMode, MaterialInfo, MeshPiece};

use super::textures::Target;
use super::MaterialContext;

/// A `dif.spec` piece's material: the `.mat`'s stand-in (base texture,
/// diffuse, vertex colour, blending) plus the family's own inputs.
pub type DifSpecMaterial = ExtendedMaterial<StandardMaterial, DifSpec>;

const SHADER: &str = "embedded://specsheet_render/difspec.wgsl";

pub struct DifSpecPlugin;

impl Plugin for DifSpecPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "difspec.wgsl");
        app.add_plugins(MaterialPlugin::<DifSpecMaterial>::default())
            .init_resource::<DifSpecMaterials>();
    }
}

// `.mat` defaults from the game's material-input table (dif-spec §3.1, C).
const DEFAULT_SPECULAR: [f32; 3] = [1.0; 3];
const DEFAULT_SHININESS: f32 = 4.0;
const DEFAULT_ENV_FACTOR: [f32; 3] = [1.0; 3];
const DEFAULT_FRESNEL: [f32; 2] = [0.2, 0.9];
// Engine inputs without a `.mat` attribute (§3.3): ours, listed in
// materials.md.
/// g, the reflection cube's brightness (H: 1).
const CUBE_BRIGHTNESS: f32 = 1.0;
/// (ms, mb), the specular-mask remap of the plain variants (H: (1, 0)).
const MASK_REMAP: [f32; 2] = [1.0, 0.0];
/// The `a` flavour discards below this base alpha × vertex alpha (§6, C).
const ALPHA_CUTOFF: f32 = 0.05;

// `DifSpecUniform::flags`, mirrored in difspec.wgsl.
const WEIGHT: u32 = 1;
const ENV: u32 = 2;
const NOFRESNEL: u32 = 4;
const REFLECTION_TEXTURE: u32 = 8;
/// `tsnmap`: the normal map on UV0 (§5.2.2).
const TSNMAP: u32 = 16;
/// `tsnmapuv`: the normal map on the "UV Normal" stream (§5.2.2).
const TSNMAPUV: u32 = 32;
/// `tsnmapuv2`: the detail map on UV0 plus the main map on the UV Normal
/// stream (§5.2.3).
const TSNMAPUV2: u32 = 64;
/// `texture_nmap` is bound.
const NORMAL_TEXTURE: u32 = 128;
/// `texture_nmap_detail` is bound.
const DETAIL_TEXTURE: u32 = 256;
/// The `dif.spec.mult.dif.spec` base (dif-spec-mult §3): `texture_mult`
/// multiplies the base, and the specular mask is `B.a·M.a`, not remapped.
const MULT: u32 = 512;
/// `texture_mult` is bound.
const MULT_TEXTURE: u32 = 1024;
/// The piece's UV slot 1 is its UV0 set, so `texture_mult` samples `UV_0`;
/// otherwise it samples `UV_1`.
const MULT_ON_UV0: u32 = 2048;
/// The piece's "UV Normal" stream is its UV0 set, so the `tsnmapuv` map
/// samples `UV_0`; set only where the mult layer takes `UV_1`.
const NORMAL_ON_UV0: u32 = 4096;

/// The spec's material inputs (dif-spec §3), in its symbols.
#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType, Reflect)]
pub struct DifSpecUniform {
    /// Ks (rgb), p.
    pub specular: Vec4,
    /// Ke (rgb), ka.
    pub environment: Vec4,
    /// fb, fs, ms, mb.
    pub fresnel: Vec4,
    /// g.
    pub reflection: Vec4,
    pub flags: u32,
}

#[derive(Asset, AsBindGroup, Reflect, Clone, Debug, Default)]
pub struct DifSpec {
    #[uniform(100)]
    pub surface: DifSpecUniform,
    /// The scene's light ([`super::GameLighting`]).
    #[storage(101, read_only)]
    pub light: Handle<ShaderBuffer>,
    /// The `.mat`'s `texture_reflection` cube.
    #[texture(102, dimension = "cube")]
    #[sampler(103)]
    pub reflection: Option<Handle<Image>>,
    /// The `.mat`'s `texture_nmap`.
    #[texture(104)]
    #[sampler(105)]
    pub normal: Option<Handle<Image>>,
    /// The `.mat`'s `texture_nmap_detail` (`tsnmapuv2`).
    #[texture(106)]
    #[sampler(107)]
    pub normal_detail: Option<Handle<Image>>,
    /// The `.mat`'s `texture_mult` (`dif.spec.mult.dif.spec`).
    #[texture(108)]
    #[sampler(109)]
    pub mult: Option<Handle<Image>>,
}

impl MaterialExtension for DifSpec {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
}

impl DifSpec {
    /// Binds the loaded reflection cube.
    pub fn bind_reflection(&mut self, image: Handle<Image>) {
        self.reflection = Some(image);
        self.surface.flags |= REFLECTION_TEXTURE;
    }
}

/// One texture of a `dif.spec` `.mat` that loads with the model (normal
/// maps, `texture_mult`): its path, and its image once
/// [`DifSpecSurface::resolve_textures`] has found it uploaded.
#[derive(Clone, Debug, PartialEq)]
struct ModelTexture {
    path: String,
    image: Option<Handle<Image>>,
}

impl ModelTexture {
    fn new(path: Option<&String>) -> Option<Self> {
        path.map(|path| Self {
            path: path.clone(),
            image: None,
        })
    }
}

/// Which base of the family an effect is.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Base {
    /// `dif.spec` and its flavours.
    Plain,
    /// `dif.spec.mult.dif.spec` (and its `iamod` sibling, drawn without
    /// the third layer).
    Mult,
    /// `dif.spec.decal.mult`: the plain maths, blended multiplicatively
    /// (dif-spec-mult §6).
    MultiplyDecal,
}

/// The `mult` base's name after `eut2.`, and the flavours it may carry:
/// those dif-spec-mult §1 covers.
const MULT_BASE: [&str; 5] = ["dif", "spec", "mult", "dif", "spec"];
const MULT_FLAVOURS: [&str; 5] = ["add", "env", "tsnmap", "tsnmapuv", "paint"];
/// The `iamod` sibling's extra base tokens, and its flavours (§5: no
/// `add.env` variant was read).
const IAMOD_BASE: [&str; 3] = ["iamod", "dif", "spec"];
const IAMOD_FLAVOURS: [&str; 2] = ["tsnmap", "tsnmapuv"];
/// The multiplicative decal's name after `eut2.` (§6).
const MULTIPLY_DECAL: [&str; 4] = ["dif", "spec", "decal", "mult"];

impl Base {
    /// `tokens` is the effect name without `eut2.` and `.rfx`. `None` for a
    /// `mult` effect the specs do not cover (`fade.mult`, `over`, `a`, …).
    fn of(tokens: &[&str]) -> Option<Self> {
        if !tokens.contains(&"mult") {
            return Some(Base::Plain);
        }
        if tokens == &MULTIPLY_DECAL[..] {
            return Some(Base::MultiplyDecal);
        }
        let flavours = tokens.strip_prefix(&MULT_BASE[..])?;
        let (flavours, allowed) = match flavours.strip_prefix(&IAMOD_BASE[..]) {
            Some(rest) => (rest, &IAMOD_FLAVOURS[..]),
            None => (flavours, &MULT_FLAVOURS[..]),
        };
        flavours
            .iter()
            .all(|t| allowed.contains(t))
            .then_some(Base::Mult)
    }
}

/// What a `dif.spec` `.mat` gives its material.
#[derive(Clone, Debug, PartialEq)]
pub struct DifSpecSurface {
    uniform: DifSpecUniform,
    reflection: Option<String>,
    normal: Option<ModelTexture>,
    normal_detail: Option<ModelTexture>,
    /// `texture_mult`, for the `mult` base.
    mult: Option<ModelTexture>,
    /// `dif.spec.decal.mult`: the lit colour multiplies the target.
    multiply_blend: bool,
}

impl DifSpecSurface {
    /// The `.mat`'s reflection cube, for `add.env` effects.
    pub fn reflection(&self) -> Option<&str> {
        self.reflection.as_deref()
    }

    /// The textures the effect samples beside its base and cube
    /// (`texture_nmap`, `texture_nmap_detail` for `tsnmapuv2`,
    /// `texture_mult`), to load with the model.
    pub fn texture_paths(&self) -> impl Iterator<Item = &str> {
        [&self.normal, &self.normal_detail, &self.mult]
            .into_iter()
            .flatten()
            .map(|texture| texture.path.as_str())
    }

    /// Whether the piece's mesh needs its tangents: the effect has a normal
    /// map and it is uploaded.
    pub fn needs_tangents(&self) -> bool {
        self.normal.as_ref().is_some_and(|map| map.image.is_some())
    }

    /// Whether the normal map samples the "UV Normal" stream (UV slot 6,
    /// §3.2): `tsnmapuv` and `tsnmapuv2`, with the map uploaded.
    fn samples_normal_uvs(&self) -> bool {
        self.needs_tangents() && self.uniform.flags & (TSNMAPUV | TSNMAPUV2) != 0
    }

    /// Whether the piece's mesh needs the "UV Normal" stream as `UV_1`. On
    /// a `mult` piece whose layer takes `UV_1`, [`Self::route_uvs`] has
    /// moved the stream to `UV_0` or onto the mult set.
    pub fn needs_normal_uvs(&self) -> bool {
        self.samples_normal_uvs()
            && self.uniform.flags & NORMAL_ON_UV0 == 0
            && !self.needs_mult_uvs()
    }

    /// Whether `texture_mult` is uploaded and samples UV slot 1 from the
    /// mesh's `UV_1` (dif-spec-mult §2.1).
    pub fn needs_mult_uvs(&self) -> bool {
        self.mult.as_ref().is_some_and(|m| m.image.is_some())
            && self.uniform.flags & MULT_ON_UV0 == 0
    }

    /// Takes the uploaded textures from `image` (by game path). A normal map
    /// that did not upload leaves the piece its vertex normal; a missing
    /// `texture_mult` leaves the layer white.
    pub fn resolve_textures(&mut self, image: impl Fn(&str) -> Option<Handle<Image>>) {
        for texture in [&mut self.normal, &mut self.normal_detail, &mut self.mult]
            .into_iter()
            .flatten()
        {
            texture.image = image(&texture.path);
        }
    }

    /// Fits the UV sets `piece` routes to the material's UV slots onto the
    /// mesh's two UV attributes; call it after [`Self::resolve_textures`].
    /// Without a `mult` layer nothing changes: the base keeps `UV_0` and the
    /// "UV Normal" stream goes to `UV_1`.
    ///
    /// The `mult` layer samples the set the piece routes to UV slot 1
    /// (dif-spec-mult §2.1), from `UV_0` when it is the UV0 set, else as
    /// `UV_1`. A `tsnmapuv` map then needs its stream on `UV_0` or equal to
    /// the mult set. If it is neither, the piece would need a third UV
    /// attribute: it keeps its vertex normal and this returns `false`.
    pub fn route_uvs(&mut self, piece: &MeshPiece) -> bool {
        // No routed slot 1: the layer has nothing to sample and stays white.
        if piece.mult_uvs.is_none() {
            if let Some(mult) = &mut self.mult {
                mult.image = None;
            }
        }
        if !self.mult.as_ref().is_some_and(|m| m.image.is_some()) {
            return true;
        }
        let on_uv0 = |uvs: &Option<Vec<[f32; 2]>>| uvs.is_some() && *uvs == piece.uvs;
        if on_uv0(&piece.mult_uvs) {
            self.uniform.flags |= MULT_ON_UV0;
            return true;
        }
        if !self.samples_normal_uvs() {
            return true;
        }
        if on_uv0(&piece.normal_uvs) {
            self.uniform.flags |= NORMAL_ON_UV0;
            return true;
        }
        if piece.normal_uvs.is_some() && piece.normal_uvs == piece.mult_uvs {
            return true;
        }
        // No "UV Normal" stream keeps the vertex normal, as without `mult`;
        // only a third distinct set is a loss.
        let routed = piece.normal_uvs.is_none();
        if let Some(normal) = &mut self.normal {
            normal.image = None;
        }
        routed
    }

    /// `None` for materials outside the family, and for those this material
    /// does not draw yet: paint, self-lit, additive effects, and `mult`
    /// effects other than the `mult` base and `dif.spec.decal.mult`.
    pub fn of(info: &MaterialInfo) -> Option<Self> {
        let name = info.effect.trim_end_matches(".rfx");
        let tokens: Vec<&str> = name.split('.').collect();
        let has = |t: &str| tokens.contains(&t);
        let family = tokens.get(1..3) == Some(&["dif", "spec"][..]);
        if !family
            || info.paintable
            || info.emissive
            || matches!(info.blend, BlendMode::Additive | BlendMode::Hidden)
        {
            return None;
        }
        let base = Base::of(tokens.get(1..)?)?;
        let env = has("env");
        let mut flags = 0;
        if base == Base::Mult {
            flags |= MULT;
        }
        if has("weight") {
            flags |= WEIGHT;
        }
        if env {
            flags |= ENV;
        }
        if has("nofresnel") {
            flags |= NOFRESNEL;
        }
        // `asafew` effects fall back to the shaders without the normal map
        // (§1, C: `dif.spec.tsnmap.asafew.a` uses those of `dif.spec.a`).
        let normal_flavour = if has("asafew") {
            0
        } else if has("tsnmapuv2") {
            TSNMAPUV2
        } else if has("tsnmapuv") {
            TSNMAPUV
        } else if has("tsnmap") {
            TSNMAP
        } else {
            0
        };
        flags |= normal_flavour;
        let specular = info.specular.unwrap_or(DEFAULT_SPECULAR);
        let factor = info.env_factor.unwrap_or(DEFAULT_ENV_FACTOR);
        let [bias, scale] = info.fresnel.unwrap_or(DEFAULT_FRESNEL);
        Some(Self {
            uniform: DifSpecUniform {
                specular: Vec3::from(specular).extend(info.shininess.unwrap_or(DEFAULT_SHININESS)),
                environment: Vec3::from(factor).extend(info.additional_ambient.unwrap_or(0.0)),
                fresnel: Vec4::new(bias, scale, MASK_REMAP[0], MASK_REMAP[1]),
                reflection: Vec4::new(CUBE_BRIGHTNESS, 0.0, 0.0, 0.0),
                flags,
            },
            reflection: info.reflection_texture.clone().filter(|_| env),
            normal: ModelTexture::new(info.normal_texture.as_ref()).filter(|_| normal_flavour != 0),
            normal_detail: ModelTexture::new(info.normal_detail_texture.as_ref())
                .filter(|_| normal_flavour == TSNMAPUV2),
            mult: ModelTexture::new(info.mult_texture.as_ref()).filter(|_| base == Base::Mult),
            multiply_blend: base == Base::MultiplyDecal,
        })
    }

    /// The uniform with the flags of the bound textures.
    fn bound_uniform(&self) -> DifSpecUniform {
        let mut uniform = self.uniform;
        for (texture, flag) in [
            (&self.normal, NORMAL_TEXTURE),
            (&self.normal_detail, DETAIL_TEXTURE),
            (&self.mult, MULT_TEXTURE),
        ] {
            if texture.as_ref().is_some_and(|t| t.image.is_some()) {
                uniform.flags |= flag;
            }
        }
        uniform
    }

    /// The `.mat` stand-in with this material's blending.
    fn standard(&self, mut standard: StandardMaterial) -> StandardMaterial {
        if let AlphaMode::Mask(_) = standard.alpha_mode {
            standard.alpha_mode = AlphaMode::Mask(ALPHA_CUTOFF);
        }
        if self.multiply_blend {
            // result = src ⊙ dst (dif-spec-mult §6.2); the shader outputs
            // alpha 1, since Bevy's multiply blend also fades by 1 − α.
            standard.alpha_mode = AlphaMode::Multiply;
            // The game fogs a `mult` draw towards white (§6.3, H), within
            // about 1 % of no fog in the garage; Bevy's fog would fade it
            // towards the scene colour and tint the target.
            standard.fog_enabled = false;
        }
        standard
    }
}

/// `dif.spec` materials, one per `.mat` stand-in and flag set: pieces of
/// one `.mat` differ only in what they bind and where their UV slots lie.
#[derive(Resource, Default)]
pub struct DifSpecMaterials {
    materials: HashMap<(AssetId<StandardMaterial>, u32), Handle<DifSpecMaterial>>,
}

impl DifSpecMaterials {
    /// The material for `base` (the `.mat`'s stand-in).
    pub fn get(
        &mut self,
        cx: &mut MaterialContext<'_>,
        materials: &mut Assets<DifSpecMaterial>,
        base: &Handle<StandardMaterial>,
        surface: &DifSpecSurface,
    ) -> Handle<DifSpecMaterial> {
        let uniform = surface.bound_uniform();
        let key = (base.id(), uniform.flags);
        if let Some(handle) = self.materials.get(&key) {
            return handle.clone();
        }
        let standard = surface.standard(cx.standard.get(base).cloned().unwrap_or_default());
        let image = |texture: &Option<ModelTexture>| texture.as_ref().and_then(|t| t.image.clone());
        let handle = materials.add(ExtendedMaterial {
            base: standard,
            extension: DifSpec {
                surface: uniform,
                light: cx.light.clone(),
                reflection: None,
                normal: image(&surface.normal),
                normal_detail: image(&surface.normal_detail),
                mult: image(&surface.mult),
            },
        });
        if let Some(cube) = &surface.reflection {
            cx.textures
                .request(cx.game, cube, Target::DifSpec(handle.clone()));
        }
        self.materials.insert(key, handle.clone());
        handle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(effect: &str) -> MaterialInfo {
        MaterialInfo {
            path: String::new(),
            effect: effect.to_owned(),
            blend: BlendMode::Opaque,
            diffuse: [1.0; 3],
            base_texture: None,
            paintable: false,
            emissive: false,
            decal: false,
            specular: None,
            shininess: None,
            env_factor: None,
            fresnel: None,
            reflection_texture: Some("/material/environment/vehicle_reflection.tobj".to_owned()),
            additional_ambient: None,
            normal_texture: None,
            normal_detail_texture: None,
            mult_texture: None,
        }
    }

    #[test]
    fn only_the_dif_spec_family_takes_this_material() {
        assert!(DifSpecSurface::of(&info("eut2.dif.spec.rfx")).is_some());
        assert!(DifSpecSurface::of(&info("eut2.dif.spec.oclu.weight.add.env.rfx")).is_some());
        for other in [
            "eut2.dif.rfx",
            "eut2.truckpaint.rfx",
            "eut2.glass.rfx",
            "eut2.dif.spec.fade.mult.dif.spec.rfx",
        ] {
            assert_eq!(DifSpecSurface::of(&info(other)), None, "{other}");
        }
        let lamp = MaterialInfo {
            emissive: true,
            ..info("eut2.dif.spec.rfx")
        };
        assert_eq!(DifSpecSurface::of(&lamp), None);
    }

    #[test]
    fn flavours_set_flags_and_absent_attributes_take_the_games_defaults() {
        let plain = DifSpecSurface::of(&info("eut2.dif.spec.rfx")).expect("dif.spec");
        assert_eq!(plain.uniform.flags, 0);
        assert_eq!(plain.uniform.specular, Vec4::new(1.0, 1.0, 1.0, 4.0));
        assert_eq!(plain.uniform.fresnel, Vec4::new(0.2, 0.9, 1.0, 0.0));
        assert_eq!(plain.reflection, None, "no cube without add.env");

        let chrome = MaterialInfo {
            env_factor: Some([0.977; 3]),
            fresnel: Some([1.0, 1.0]),
            shininess: Some(70.0),
            additional_ambient: Some(0.5),
            ..info("eut2.dif.spec.weight.add.env.rfx")
        };
        let chrome = DifSpecSurface::of(&chrome).expect("dif.spec");
        assert_eq!(chrome.uniform.flags, WEIGHT | ENV);
        assert_eq!(
            chrome.uniform.environment,
            Vec4::new(0.977, 0.977, 0.977, 0.5)
        );
        assert_eq!(chrome.uniform.specular.w, 70.0);
        assert!(chrome.reflection.is_some());

        let flat =
            DifSpecSurface::of(&info("eut2.dif.spec.add.env.nofresnel.rfx")).expect("dif.spec");
        assert_eq!(flat.uniform.flags, ENV | NOFRESNEL);
    }

    fn normal_mapped(effect: &str) -> MaterialInfo {
        MaterialInfo {
            normal_texture: Some("/vehicle/truck/x/nmap.tobj".to_owned()),
            normal_detail_texture: Some("/vehicle/truck/x/nmap_detail.tobj".to_owned()),
            ..info(effect)
        }
    }

    #[test]
    fn normal_map_flavours_set_their_flag_and_name_their_maps() {
        let cases = [
            ("eut2.dif.spec.tsnmap.rfx", TSNMAP, 1),
            (
                "eut2.dif.spec.weight.add.env.tsnmapuv.rfx",
                WEIGHT | ENV | TSNMAPUV,
                1,
            ),
            ("eut2.dif.spec.tsnmapuv2.rfx", TSNMAPUV2, 2),
        ];
        for (effect, flags, maps) in cases {
            let surface = DifSpecSurface::of(&normal_mapped(effect)).expect("dif.spec");
            assert_eq!(surface.uniform.flags, flags, "{effect}");
            assert_eq!(surface.texture_paths().count(), maps, "{effect}");
        }

        // Without the flavour, or with `asafew` (§1: its shaders are those
        // without the normal map), the maps are not used.
        for effect in ["eut2.dif.spec.rfx", "eut2.dif.spec.tsnmap.asafew.a.rfx"] {
            let surface = DifSpecSurface::of(&normal_mapped(effect)).expect("dif.spec");
            assert_eq!(surface.uniform.flags & (TSNMAP | TSNMAPUV | TSNMAPUV2), 0);
            assert_eq!(surface.texture_paths().count(), 0, "{effect}");
        }
    }

    #[test]
    fn only_an_uploaded_normal_map_asks_for_tangents_and_normal_uvs() {
        let mut uv =
            DifSpecSurface::of(&normal_mapped("eut2.dif.spec.tsnmapuv.rfx")).expect("dif.spec");
        assert!(!uv.needs_tangents(), "nothing uploaded yet");
        uv.resolve_textures(|_| None);
        assert!(!uv.needs_tangents(), "the upload failed");
        uv.resolve_textures(|_| Some(Handle::default()));
        assert!(uv.needs_tangents() && uv.needs_normal_uvs());

        let mut base =
            DifSpecSurface::of(&normal_mapped("eut2.dif.spec.tsnmap.rfx")).expect("dif.spec");
        base.resolve_textures(|_| Some(Handle::default()));
        assert!(base.needs_tangents());
        assert!(!base.needs_normal_uvs(), "tsnmap samples UV0");

        let missing = MaterialInfo {
            normal_texture: None,
            ..normal_mapped("eut2.dif.spec.tsnmap.rfx")
        };
        let mut missing = DifSpecSurface::of(&missing).expect("dif.spec");
        missing.resolve_textures(|_| Some(Handle::default()));
        assert!(!missing.needs_tangents(), "no texture_nmap");
    }

    fn mult(effect: &str) -> MaterialInfo {
        MaterialInfo {
            mult_texture: Some("/model/ui/garage/ao_textures/x.tobj".to_owned()),
            ..normal_mapped(effect)
        }
    }

    #[test]
    fn the_mult_base_is_a_flavour_and_names_its_texture() {
        let cases = [
            ("eut2.dif.spec.mult.dif.spec.rfx", MULT),
            (
                "eut2.dif.spec.mult.dif.spec.add.env.tsnmapuv.rfx",
                MULT | ENV | TSNMAPUV,
            ),
            ("eut2.dif.spec.mult.dif.spec.add.env.paint.rfx", MULT | ENV),
            (
                "eut2.dif.spec.mult.dif.spec.iamod.dif.spec.tsnmap.rfx",
                MULT | TSNMAP,
            ),
        ];
        for (effect, flags) in cases {
            let surface = DifSpecSurface::of(&mult(effect)).expect("mult base");
            assert_eq!(surface.uniform.flags, flags, "{effect}");
            assert!(
                surface.mult.is_some() && !surface.multiply_blend,
                "{effect}"
            );
        }
        let plates = DifSpecSurface::of(&mult("eut2.dif.spec.mult.dif.spec.tsnmapuv.rfx"))
            .expect("mult base");
        assert_eq!(plates.texture_paths().count(), 2, "nmap and mult");

        // Plain `dif.spec` has no `texture_mult` (dif-spec §3.1).
        let plain = DifSpecSurface::of(&mult("eut2.dif.spec.rfx")).expect("dif.spec");
        assert_eq!(plain.mult, None);

        // Flavours and siblings the specs do not cover keep the stand-in.
        for other in [
            "eut2.dif.spec.mult.dif.spec.a.rfx",
            "eut2.dif.spec.mult.dif.spec.over.rfx",
            "eut2.dif.spec.mult.dif.spec.tsnmapuv2.rfx",
            "eut2.dif.spec.mult.dif.spec.iamod.dif.spec.add.env.rfx",
            "eut2.dif.spec.fade.mult.dif.spec.rfx",
            "eut2.dif.spec.mult.rfx",
        ] {
            assert_eq!(DifSpecSurface::of(&mult(other)), None, "{other}");
        }
    }

    #[test]
    fn the_multiplicative_decal_is_plain_dif_spec_blended_by_multiplying() {
        let decal = MaterialInfo {
            blend: BlendMode::Blend,
            decal: true,
            ..mult("eut2.dif.spec.decal.mult.rfx")
        };
        let decal = DifSpecSurface::of(&decal).expect("decal.mult");
        assert_eq!(decal.uniform.flags, 0, "the plain maths (§6.1)");
        assert_eq!(decal.mult, None);
        assert!(decal.multiply_blend);
        let standard = decal.standard(StandardMaterial {
            alpha_mode: AlphaMode::Blend,
            ..default()
        });
        assert_eq!(standard.alpha_mode, AlphaMode::Multiply);
        assert!(!standard.fog_enabled);

        let over = MaterialInfo {
            blend: BlendMode::Blend,
            ..info("eut2.dif.spec.over.rfx")
        };
        let over = DifSpecSurface::of(&over).expect("dif.spec");
        let standard = over.standard(StandardMaterial {
            alpha_mode: AlphaMode::Blend,
            ..default()
        });
        assert_eq!(standard.alpha_mode, AlphaMode::Blend);
        assert!(standard.fog_enabled);
    }

    const SET_0: [f32; 2] = [4.0, -3.0];
    const SET_1: [f32; 2] = [0.25, 0.75];
    const SET_2: [f32; 2] = [0.5, 0.5];

    /// A triangle that routes UV slot 1 to `mult` and slot 6 to `normal`.
    fn piece(mult: Option<[f32; 2]>, normal: Option<[f32; 2]>) -> MeshPiece {
        MeshPiece {
            material: 0,
            positions: vec![[0.0; 3]; 3],
            normals: vec![[0.0, 1.0, 0.0]; 3],
            uvs: Some(vec![SET_0; 3]),
            paint_uvs: None,
            alternate_paint_uvs: None,
            colors: None,
            normal_uvs: normal.map(|uv| vec![uv; 3]),
            mult_uvs: mult.map(|uv| vec![uv; 3]),
            tangents: Some(vec![[1.0, 0.0, 0.0, 1.0]; 3]),
            indices: vec![0, 1, 2],
        }
    }

    /// The garage plates' effect with every texture uploaded, routed for
    /// `piece`.
    fn route(effect: &str, piece: &MeshPiece) -> (DifSpecSurface, bool) {
        let mut surface = DifSpecSurface::of(&mult(effect)).expect("dif.spec");
        surface.resolve_textures(|_| Some(Handle::default()));
        let routed = surface.route_uvs(piece);
        (surface, routed)
    }

    const PLATES: &str = "eut2.dif.spec.mult.dif.spec.add.env.tsnmapuv.rfx";

    #[test]
    fn the_mult_layer_takes_uv_1_and_the_normal_map_moves_to_uv0() {
        // The garage (dif-spec-mult §2.1): slot 1 → set 1, slot 6 → set 0.
        let (garage, routed) = route(PLATES, &piece(Some(SET_1), Some(SET_0)));
        assert!(routed);
        assert!(garage.needs_mult_uvs() && !garage.needs_normal_uvs());
        assert!(garage.needs_tangents());
        assert_eq!(
            garage.uniform.flags & (MULT_ON_UV0 | NORMAL_ON_UV0),
            NORMAL_ON_UV0
        );
        assert_ne!(garage.bound_uniform().flags & MULT_TEXTURE, 0);

        // Slot 1 from set 0 (most `mult` pieces): the layer reads UV0 and
        // the normal map keeps UV_1.
        let (on_uv0, routed) = route(PLATES, &piece(Some(SET_0), Some(SET_2)));
        assert!(routed);
        assert!(!on_uv0.needs_mult_uvs() && on_uv0.needs_normal_uvs());
        assert_eq!(
            on_uv0.uniform.flags & (MULT_ON_UV0 | NORMAL_ON_UV0),
            MULT_ON_UV0
        );

        // Both on one set: UV_1 serves both.
        let (shared, routed) = route(PLATES, &piece(Some(SET_1), Some(SET_1)));
        assert!(routed);
        assert!(shared.needs_mult_uvs() && !shared.needs_normal_uvs());
        assert!(shared.needs_tangents());
        assert_eq!(shared.uniform.flags & (MULT_ON_UV0 | NORMAL_ON_UV0), 0);
    }

    #[test]
    fn three_distinct_uv_sets_drop_the_normal_map_and_a_missing_slot_drops_the_layer() {
        let (three, routed) = route(PLATES, &piece(Some(SET_1), Some(SET_2)));
        assert!(!routed, "a third UV attribute would be needed");
        assert!(three.needs_mult_uvs());
        assert!(!three.needs_tangents() && !three.needs_normal_uvs());
        assert_eq!(three.bound_uniform().flags & NORMAL_TEXTURE, 0);

        // No "UV Normal" stream: the vertex normal, as without `mult`.
        let (no_stream, routed) = route(PLATES, &piece(Some(SET_1), None));
        assert!(routed);
        assert!(no_stream.needs_mult_uvs() && !no_stream.needs_tangents());

        // No slot 1: the layer stays white and asks for no UVs.
        let (no_slot, routed) = route(PLATES, &piece(None, Some(SET_2)));
        assert!(routed);
        assert!(!no_slot.needs_mult_uvs() && no_slot.needs_normal_uvs());
        assert_eq!(no_slot.bound_uniform().flags & MULT_TEXTURE, 0);
    }

    #[test]
    fn routing_leaves_pieces_without_a_mult_layer_as_they_were() {
        let effect = "eut2.dif.spec.tsnmapuv.rfx";
        let mut plain = DifSpecSurface::of(&normal_mapped(effect)).expect("dif.spec");
        plain.resolve_textures(|_| Some(Handle::default()));
        let before = plain.clone();
        assert!(plain.route_uvs(&piece(Some(SET_1), Some(SET_0))));
        assert_eq!(plain, before);
        assert!(plain.needs_normal_uvs() && !plain.needs_mult_uvs());

        // A `mult` effect whose texture did not upload draws a white layer.
        let mut failed = DifSpecSurface::of(&mult(PLATES)).expect("mult base");
        failed.resolve_textures(|path| (!path.contains("ao_textures")).then(Handle::default));
        assert!(failed.route_uvs(&piece(Some(SET_1), Some(SET_2))));
        assert!(!failed.needs_mult_uvs() && failed.needs_normal_uvs());
    }
}
