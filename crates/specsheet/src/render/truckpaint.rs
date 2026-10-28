//! The `eut2.truckpaint` effect family as a Bevy material, from the spec
//! `docs/app/shader-specs/truckpaint.md`. The paint's surface follows the
//! spec: albedo, colour masks, airbrush liveries, flip colour, flakes and
//! the mask's specular factor. The lighting is still Bevy's PBR, shared with
//! the materials next to it, until the game's lighting model replaces it for
//! every material at once (`docs/app/materials.md`).

use std::collections::HashMap;
use std::sync::Arc;

use bevy::asset::embedded_asset;
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::render::renderer::RenderDevice;
use bevy::render::settings::WgpuFeatures;
use bevy::shader::ShaderRef;
use bevy::tasks::{futures::check_ready, AsyncComputeTaskPool, Task};
use specsheet_data::paint::{Mask, PiecePaint};
use specsheet_data::render::{load_texture, MaterialInfo, TextureData};
use specsheet_data::session::GameSession;

use crate::assets;

/// A `truckpaint` piece's material: the `.mat`'s PBR stand-in plus the paint.
pub type TruckPaintMaterial = ExtendedMaterial<StandardMaterial, TruckPaint>;

const SHADER: &str = "embedded://specsheet/render/truckpaint.wgsl";

pub struct TruckPaintPlugin;

impl Plugin for TruckPaintPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "truckpaint.wgsl");
        app.add_plugins(MaterialPlugin::<TruckPaintMaterial>::default())
            .init_resource::<PaintMaterials>()
            .add_systems(Update, finish_paint_textures);
    }
}

// Defaults for the spec's Unknowns: ours, each listed in materials.md.
/// λ, the flake light's strength, is engine-set (truckpaint.md §7).
const FLAKE_STRENGTH: f32 = 1.0;
/// φ above 1 makes the facing weight negative (§7: `flip_strength` reaches
/// 2.7), so it is clamped to the blend's own range.
const MAX_FLIP_WEIGHT: f32 = 1.0;
/// ρ above 1 drives the flake coverage below 0 (§7: density reaches 2).
const MAX_FLAKE_DENSITY: f32 = 1.0;
/// `flake_vratio` 0 would collapse the noise's v scale (§7); 0 or missing
/// means an unstretched noise.
const DEFAULT_VRATIO: f32 = 1.0;
/// F0 when a `.mat` has no `fresnel`: the bias of the seen pair {0.2, 0.9}.
/// Which of the pair the effect reads is Unknown (lighting §8); we take the
/// first.
const DEFAULT_FRESNEL: f32 = 0.2;
/// `.mat` `specular` and `shininess` when absent. 130 keeps the perceptual
/// roughness (0.35) the paint had before this material.
const DEFAULT_SPECULAR: [f32; 3] = [1.0; 3];
const DEFAULT_SHININESS: f32 = 130.0;
/// The paint colour when neither the paint job nor the configuration has one.
const NEUTRAL_PAINT: [f32; 3] = [0.6; 3];

// `PaintUniform::flags`, mirrored in truckpaint.wgsl.
const COLORMASK: u32 = 1;
const AIRBRUSH: u32 = 2;
const FLIPFLAKE: u32 = 4;
const ALTUV: u32 = 8;
const MASK_TEXTURE: u32 = 16;
const FLAKE_TEXTURE: u32 = 32;

/// The spec's paint inputs (truckpaint.md §2), in its symbols.
#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType, Reflect)]
pub struct PaintUniform {
    /// P, the paint base colour (rgb).
    pub base: Vec4,
    /// Q_r, Q_g, Q_b (rgb).
    pub mask_r: Vec4,
    pub mask_g: Vec4,
    pub mask_b: Vec4,
    /// A uniform mask (linear rgb, alpha) used while no texture is bound.
    pub mask_constant: Vec4,
    /// K_flip (rgb), φ.
    pub flip: Vec4,
    /// K_flake (rgb), ρ.
    pub flake: Vec4,
    /// s, p_f, k, r.
    pub flake_setup: Vec4,
    /// S_mat (rgb), p_mat.
    pub specular: Vec4,
    /// F0, λ.
    pub shading: Vec4,
    pub flags: u32,
}

#[derive(Asset, AsBindGroup, Reflect, Clone, Debug, Default)]
pub struct TruckPaint {
    #[uniform(100)]
    pub paint: PaintUniform,
    /// `texture_paintjob`: the mask or the livery.
    #[texture(101)]
    #[sampler(102)]
    pub mask: Option<Handle<Image>>,
    /// `texture_flakenoise`.
    #[texture(103)]
    #[sampler(104)]
    pub flake_noise: Option<Handle<Image>>,
}

impl MaterialExtension for TruckPaint {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
}

/// What a `truckpaint` `.mat` contributes: the flavours its effect names
/// that the paint uses (`airbrush` is the paint job's choice) and its
/// specular inputs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaintSurface {
    flipflake: bool,
    altuv: bool,
    specular: [f32; 3],
    shininess: f32,
    fresnel: f32,
}

impl PaintSurface {
    /// `None` for materials that are not `truckpaint`.
    pub fn of(info: &MaterialInfo) -> Option<Self> {
        if !info.paintable {
            return None;
        }
        let tokens: Vec<&str> = info.effect.trim_end_matches(".rfx").split('.').collect();
        let has = |token: &str| tokens.contains(&token);
        Some(Self {
            flipflake: has("flipflake") || has("flipflakeuv"),
            altuv: has("altuv"),
            specular: info.specular.unwrap_or(DEFAULT_SPECULAR),
            shininess: info.shininess.unwrap_or(DEFAULT_SHININESS),
            fresnel: info.fresnel.map_or(DEFAULT_FRESNEL, |f| f[0]),
        })
    }
}

/// The paint a placed accessory takes on its `truckpaint` pieces.
pub enum Paint<'a> {
    /// The paint job's resolved paint (`specsheet_data::paint`).
    Job(&'a PiecePaint),
    /// No paint job: the configuration's colour, if any.
    Color(Option<[f32; 3]>),
}

impl Paint<'_> {
    /// Identifies the paint in instance and material keys.
    pub fn key(&self) -> String {
        match self {
            Paint::Job(paint) => format!("{paint:?}"),
            Paint::Color(color) => format!("color {color:?}"),
        }
    }

    /// P. Game colours are linear (Coffee Brown is (0.034, 0.023, 0.014)).
    fn base_color(&self) -> [f32; 3] {
        match self {
            Paint::Job(paint) => paint.base_color,
            Paint::Color(color) => *color,
        }
        .unwrap_or(NEUTRAL_PAINT)
    }
}

fn rgb_w([r, g, b]: [f32; 3], w: f32) -> Vec4 {
    Vec4::new(r, g, b, w)
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// The uniform for `surface` painted with `paint`. `altuv` is the union of
/// the `.mat`'s flavour and the paint job's flag (Altamira on the MAN TGX
/// Euro 6 cab needs the `.mat`'s); `flipflake` likewise (Hypothesis, pending
/// REA). Livery or colour mask is the paint job's `airbrush` alone: a `.mat`
/// naming `airbrush` (the MAN TGX Euro 6 cab) still shows colour-mask jobs as
/// colour masks, and a flipflake job is never a colour mask. Texture flags
/// are set when the textures are bound.
pub fn paint_uniform(surface: &PaintSurface, paint: &Paint<'_>) -> PaintUniform {
    let base = paint.base_color();
    let mut uniform = PaintUniform {
        base: rgb_w(base, 1.0),
        mask_r: rgb_w(base, 1.0),
        mask_g: rgb_w(base, 1.0),
        mask_b: rgb_w(base, 1.0),
        specular: rgb_w(surface.specular, surface.shininess),
        shading: Vec4::new(surface.fresnel, FLAKE_STRENGTH, 0.0, 0.0),
        flags: if surface.altuv { ALTUV } else { 0 },
        ..default()
    };
    let Paint::Job(job) = paint else {
        return uniform;
    };
    if job.alternate_uvset {
        uniform.flags |= ALTUV;
    }
    let flipflake = job.flipflake || surface.flipflake;
    // The game never combines colormask with flipflake: a metallic job's
    // mask is used only as a livery.
    let mask = job.mask.as_ref().filter(|_| job.airbrush || !flipflake);
    if let Some(mask) = mask {
        let airbrush = job.airbrush;
        uniform.flags |= if airbrush { AIRBRUSH } else { COLORMASK };
        let [r, g, b] = job.mask_colors.map(|c| rgb_w(c.unwrap_or(base), 1.0));
        (uniform.mask_r, uniform.mask_g, uniform.mask_b) = (r, g, b);
        uniform.mask_constant = match mask {
            Mask::Constant { srgba, .. } => {
                let [r, g, b, a] = *srgba;
                Vec4::new(srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b), a)
            }
            // Until the texture is bound (or when it fails): the plain paint,
            // with the colour mask's full specular.
            Mask::Texture(_) if airbrush => Vec4::ZERO,
            Mask::Texture(_) => Vec4::W,
        };
    }
    if flipflake {
        uniform.flags |= FLIPFLAKE;
        let flip = job.flip.strength.unwrap_or(0.0).clamp(0.0, MAX_FLIP_WEIGHT);
        uniform.flip = rgb_w(job.flip.color.unwrap_or(base), flip);
        let flake = &job.flake;
        let density = flake.density.unwrap_or(0.0).clamp(0.0, MAX_FLAKE_DENSITY);
        uniform.flake = rgb_w(flake.color.unwrap_or([1.0; 3]), density);
        uniform.flake_setup = Vec4::new(
            flake.uvscale.unwrap_or(1.0),
            flake.shininess.unwrap_or(surface.shininess),
            flake.clearcoat_rolloff.unwrap_or(0.0),
            flake.vratio.filter(|r| *r > 0.0).unwrap_or(DEFAULT_VRATIO),
        );
    }
    uniform
}

/// The two textures a paint can bind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Binding {
    Mask,
    FlakeNoise,
}

enum TextureState {
    Loading(Task<Result<TextureData, String>>),
    Ready(Handle<Image>),
    Failed,
}

/// Paint materials, shared by every piece with the same `.mat` and paint,
/// and the mask and flake textures they bind. A texture that is loading or
/// failed leaves its binding empty and its flag clear, so the piece draws
/// with the paint colour alone.
#[derive(Resource, Default)]
pub struct PaintMaterials {
    materials: HashMap<(AssetId<StandardMaterial>, String), Handle<TruckPaintMaterial>>,
    textures: HashMap<String, TextureState>,
    waiting: Vec<(Handle<TruckPaintMaterial>, String, Binding)>,
}

impl PaintMaterials {
    /// The material for `base` (the `.mat`'s stand-in) under `paint`.
    pub fn get(
        &mut self,
        game: Option<&Arc<GameSession>>,
        standard: &Assets<StandardMaterial>,
        materials: &mut Assets<TruckPaintMaterial>,
        base: &Handle<StandardMaterial>,
        surface: &PaintSurface,
        paint: &Paint<'_>,
    ) -> Handle<TruckPaintMaterial> {
        let key = (base.id(), paint.key());
        if let Some(handle) = self.materials.get(&key) {
            return handle.clone();
        }
        let handle = materials.add(ExtendedMaterial {
            base: standard.get(base).cloned().unwrap_or_default(),
            extension: TruckPaint {
                paint: paint_uniform(surface, paint),
                ..default()
            },
        });
        if let Paint::Job(job) = paint {
            let flags = paint_uniform(surface, paint).flags;
            if let Some(Mask::Texture(path)) = &job.mask {
                if flags & (COLORMASK | AIRBRUSH) != 0 {
                    self.request(game, &handle, path, Binding::Mask);
                }
            }
            if flags & FLIPFLAKE != 0 {
                self.request(game, &handle, &job.flake.noise, Binding::FlakeNoise);
            }
        }
        self.materials.insert(key, handle.clone());
        handle
    }

    fn request(
        &mut self,
        game: Option<&Arc<GameSession>>,
        material: &Handle<TruckPaintMaterial>,
        path: &str,
        binding: Binding,
    ) {
        if !self.textures.contains_key(path) {
            let Some(game) = game.cloned() else {
                return;
            };
            let task_path = path.to_owned();
            let task = AsyncComputeTaskPool::get().spawn(async move {
                load_texture(game.fs(), &task_path).map_err(|e| e.to_string())
            });
            self.textures
                .insert(path.to_owned(), TextureState::Loading(task));
        }
        self.waiting
            .push((material.clone(), path.to_owned(), binding));
    }
}

/// Uploads finished paint textures and binds them to the materials
/// waiting for them.
fn finish_paint_textures(
    mut paints: ResMut<PaintMaterials>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<TruckPaintMaterial>>,
    device: Option<Res<RenderDevice>>,
) {
    if paints.waiting.is_empty() {
        return;
    }
    let bc_supported =
        device.is_some_and(|d| d.features().contains(WgpuFeatures::TEXTURE_COMPRESSION_BC));
    let paints = &mut *paints;
    for (path, state) in &mut paints.textures {
        let TextureState::Loading(task) = state else {
            continue;
        };
        let Some(result) = check_ready(task) else {
            continue;
        };
        *state = match result.and_then(|data| assets::texture_image(data, bc_supported)) {
            Ok(image) => TextureState::Ready(images.add(image)),
            Err(error) => {
                warn!("paint texture {path}: {error}");
                TextureState::Failed
            }
        };
    }
    paints.waiting.retain(|(material, path, binding)| {
        let image = match paints.textures.get(path) {
            Some(TextureState::Loading(_)) => return true,
            Some(TextureState::Ready(image)) => image.clone(),
            Some(TextureState::Failed) | None => return false,
        };
        if let Some(mut material) = materials.get_mut(material) {
            let paint = &mut material.extension;
            match binding {
                Binding::Mask => {
                    paint.mask = Some(image);
                    paint.paint.flags |= MASK_TEXTURE;
                }
                Binding::FlakeNoise => {
                    paint.flake_noise = Some(image);
                    paint.paint.flags |= FLAKE_TEXTURE;
                }
            }
        }
        false
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use specsheet_core::AccessoryId;
    use specsheet_data::paint::{FlakePaint, FlipPaint};

    fn surface(effect: &str) -> PaintSurface {
        PaintSurface {
            flipflake: effect.contains("flipflake"),
            altuv: effect.contains("altuv"),
            specular: [0.5; 3],
            shininess: 60.0,
            fresnel: 0.2,
        }
    }

    fn job(mask: Option<Mask>) -> PiecePaint {
        PiecePaint {
            paint_job: AccessoryId::from("stripes.t.paint_job"),
            part: None,
            base_color: Some([0.1, 0.2, 0.3]),
            mask,
            mask_colors: [Some([1.0, 0.0, 0.0]), None, Some([0.0, 0.0, 1.0])],
            flake: FlakePaint {
                color: None,
                density: Some(2.0),
                shininess: None,
                clearcoat_rolloff: Some(3.0),
                uvscale: Some(8.0),
                vratio: Some(0.0),
                noise: "/material/custom/flake_noise.tobj".to_owned(),
            },
            flip: FlipPaint {
                color: Some([0.9, 0.8, 0.7]),
                strength: Some(2.7),
            },
            airbrush: false,
            alternate_uvset: false,
            flipflake: false,
        }
    }

    #[test]
    fn a_plain_colour_paints_the_base_only() {
        let u = paint_uniform(&surface("truckpaint"), &Paint::Color(None));
        assert_eq!(u.base, rgb_w(NEUTRAL_PAINT, 1.0));
        assert_eq!(u.flags, 0);
        assert_eq!(u.specular, Vec4::new(0.5, 0.5, 0.5, 60.0));
        let alt = paint_uniform(&surface("truckpaint.altuv"), &Paint::Color(None));
        assert_eq!(alt.flags, ALTUV, "the .mat's flavour applies without a job");
    }

    #[test]
    fn a_mask_becomes_a_colour_mask_or_a_livery() {
        let paint = job(Some(Mask::Texture("/m.tobj".to_owned())));
        let u = paint_uniform(&surface("truckpaint"), &Paint::Job(&paint));
        assert_eq!(u.flags, COLORMASK);
        assert_eq!(u.mask_r, Vec4::new(1.0, 0.0, 0.0, 1.0));
        assert_eq!(u.mask_g, Vec4::new(0.1, 0.2, 0.3, 1.0), "missing: P");
        let livery = PiecePaint {
            alternate_uvset: true,
            ..paint.clone()
        };
        let u = paint_uniform(&surface("truckpaint"), &Paint::Job(&livery));
        assert_eq!(u.flags, COLORMASK | ALTUV);
        let airbrush = PiecePaint {
            airbrush: true,
            ..paint.clone()
        };
        let u = paint_uniform(&surface("truckpaint"), &Paint::Job(&airbrush));
        assert_eq!(u.flags, AIRBRUSH);
    }

    #[test]
    fn the_paint_job_not_the_mat_decides_livery_or_colour_mask() {
        // The MAN TGX Euro 6 cab's .mat names `airbrush`, yet Driving
        // Academy is a colour-mask job: drawn as a livery, its channel
        // weights showed as red and green stripes.
        let academy = job(Some(Mask::Texture("/m.tobj".to_owned())));
        let u = paint_uniform(
            &surface("truckpaint.tsnmapuv.airbrush.altuv"),
            &Paint::Job(&academy),
        );
        assert_eq!(u.flags, COLORMASK | ALTUV);
    }

    #[test]
    fn a_flipflake_job_never_draws_a_colour_mask() {
        // The game's paint-material builder sets colormask only without
        // flipflake and airbrush (research/evidence/rea-paint-variant.txt).
        let metallic = PiecePaint {
            flipflake: true,
            ..job(Some(Mask::Texture("/m.tobj".to_owned())))
        };
        let u = paint_uniform(&surface("truckpaint"), &Paint::Job(&metallic));
        assert_eq!(u.flags, FLIPFLAKE);
        let livery = PiecePaint {
            airbrush: true,
            ..metallic
        };
        let u = paint_uniform(&surface("truckpaint"), &Paint::Job(&livery));
        assert_eq!(u.flags, AIRBRUSH | FLIPFLAKE);
    }

    #[test]
    fn a_constant_mask_enters_linear() {
        let paint = job(Some(Mask::Constant {
            srgba: [1.0, 0.5, 0.0, 0.25],
            path: String::new(),
        }));
        let u = paint_uniform(&surface("truckpaint"), &Paint::Job(&paint));
        assert!((u.mask_constant.y - 0.214).abs() < 1e-3);
        assert_eq!((u.mask_constant.x, u.mask_constant.w), (1.0, 0.25));
    }

    #[test]
    fn flip_and_flake_inputs_stay_in_range() {
        let paint = PiecePaint {
            flipflake: true,
            ..job(None)
        };
        let u = paint_uniform(&surface("truckpaint"), &Paint::Job(&paint));
        assert_eq!(u.flags, FLIPFLAKE);
        assert_eq!(u.flip, Vec4::new(0.9, 0.8, 0.7, MAX_FLIP_WEIGHT));
        assert_eq!(u.flake, Vec4::new(1.0, 1.0, 1.0, MAX_FLAKE_DENSITY));
        assert_eq!(u.flake_setup, Vec4::new(8.0, 60.0, 3.0, DEFAULT_VRATIO));
    }

    #[test]
    fn only_truckpaint_materials_have_a_paint_surface() {
        let mut info = MaterialInfo {
            path: String::new(),
            effect: "eut2.truckpaint.tsnmapuv.airbrush.altuv.rfx".to_owned(),
            blend: specsheet_data::render::BlendMode::Opaque,
            diffuse: [1.0; 3],
            base_texture: None,
            paintable: true,
            emissive: false,
            decal: false,
            specular: None,
            shininess: None,
            env_factor: None,
            fresnel: Some([0.3, 0.9]),
            reflection_texture: None,
        };
        let s = PaintSurface::of(&info).expect("paint");
        assert!(s.altuv && !s.flipflake);
        assert_eq!((s.fresnel, s.shininess), (0.3, DEFAULT_SHININESS));
        info.paintable = false;
        assert_eq!(PaintSurface::of(&info), None);
    }
}
