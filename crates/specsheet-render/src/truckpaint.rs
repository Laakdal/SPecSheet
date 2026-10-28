//! The `eut2.truckpaint` effect family as a Bevy material, from the specs
//! `docs/app/shader-specs/truckpaint.md` and `truckpaint-lighting.md`: the
//! paint's surface (albedo, colour masks, airbrush liveries, flip colour,
//! flakes, the mask's specular factor), lit by the game's forward model
//! (`game_lighting.wgsl`) with the `.mat`'s own reflection cube.

use std::collections::HashMap;

use bevy::asset::embedded_asset;
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::render::storage::ShaderBuffer;
use bevy::shader::ShaderRef;
use specsheet_core::colour::srgb_to_linear;
use specsheet_data::paint::{Mask, PiecePaint};
use specsheet_data::render::MaterialInfo;

use super::textures::Target;
use super::MaterialContext;

/// A `truckpaint` piece's material: the `.mat`'s stand-in (base texture,
/// diffuse, vertex colour) plus the paint.
pub type TruckPaintMaterial = ExtendedMaterial<StandardMaterial, TruckPaint>;

const SHADER: &str = "embedded://specsheet_render/truckpaint.wgsl";

pub struct TruckPaintPlugin;

impl Plugin for TruckPaintPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "truckpaint.wgsl");
        app.add_plugins(MaterialPlugin::<TruckPaintMaterial>::default())
            .init_resource::<PaintMaterials>();
    }
}

// Defaults for the spec's Unknowns: ours, each listed in materials.md.
/// λ, the flake light's strength, is engine-set (truckpaint.md §7).
const FLAKE_STRENGTH: f32 = 1.0;
/// μ, `texture_reflection_modulator`, is engine-set (lighting §4).
const REFLECTION_MODULATOR: f32 = 1.0;
/// φ above 1 makes the facing weight negative (§7: `flip_strength` reaches
/// 2.7), so it is clamped to the blend's own range.
const MAX_FLIP_WEIGHT: f32 = 1.0;
/// ρ above 1 drives the flake coverage below 0 (§7: density reaches 2).
const MAX_FLAKE_DENSITY: f32 = 1.0;
/// `flake_vratio` 0 would collapse the noise's v scale (§7), so 0 means an
/// unstretched noise.
const ZERO_VRATIO: f32 = 1.0;
/// F0 when a `.mat` has no `fresnel`: the bias of the seen pair {0.2, 0.9}.
/// Which of the pair the effect reads is Unknown (lighting §8); we take the
/// first.
const DEFAULT_FRESNEL: f32 = 0.2;
/// `.mat` `specular` and `shininess` when absent: the game's material-input
/// table defaults (dif-spec.md §3.1; Hypothesis for truckpaint).
const DEFAULT_SPECULAR: [f32; 3] = [1.0; 3];
const DEFAULT_SHININESS: f32 = 4.0;
/// The paint colour without a paint job when the configuration has none.
const NEUTRAL_PAINT: [f32; 3] = [0.6; 3];

// `PaintUniform::flags`, mirrored in truckpaint.wgsl.
const COLORMASK: u32 = 1;
const AIRBRUSH: u32 = 2;
const FLIPFLAKE: u32 = 4;
const MASK_TEXTURE: u32 = 16;
const FLAKE_TEXTURE: u32 = 32;
const REFLECTION_TEXTURE: u32 = 64;

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
    /// F0, λ, a_mat, μ.
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
    /// The scene's light ([`crate::GameLighting`]).
    #[storage(105, read_only)]
    pub light: Handle<ShaderBuffer>,
    /// The `.mat`'s `texture_reflection` cube.
    #[texture(106, dimension = "cube")]
    #[sampler(107)]
    pub reflection: Option<Handle<Image>>,
}

impl MaterialExtension for TruckPaint {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
}

/// What a `truckpaint` `.mat` contributes: its specular, ambient and
/// reflection inputs. The flavours it names (`airbrush`, `flipflake`,
/// `altuv`) are replaced by the paint job's
/// (research/evidence/rea-paint-variant.txt).
#[derive(Clone, Debug, PartialEq)]
pub struct PaintSurface {
    specular: [f32; 3],
    shininess: f32,
    fresnel: f32,
    additional_ambient: f32,
    reflection: Option<String>,
}

impl PaintSurface {
    /// `None` for materials that are not `truckpaint`.
    pub fn of(info: &MaterialInfo) -> Option<Self> {
        if !info.paintable {
            return None;
        }
        Some(Self {
            specular: info.specular.unwrap_or(DEFAULT_SPECULAR),
            shininess: info.shininess.unwrap_or(DEFAULT_SHININESS),
            fresnel: info.fresnel.map_or(DEFAULT_FRESNEL, |f| f[0]),
            additional_ambient: info.additional_ambient.unwrap_or(0.0),
            reflection: info.reflection_texture.clone(),
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
    /// True when the paint samples the alternate UV set
    /// ([`specsheet_data::render::paint_uv_set`]).
    pub fn alternate_uvset(&self) -> bool {
        matches!(self, Paint::Job(job) if job.alternate_uvset)
    }

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
            Paint::Color(color) => color.unwrap_or(NEUTRAL_PAINT),
        }
    }
}

fn rgb_w([r, g, b]: [f32; 3], w: f32) -> Vec4 {
    Vec4::new(r, g, b, w)
}

/// The uniform for `surface` painted with `paint`. The variant follows the
/// paint job alone, as the game's paint-material builder does: livery
/// (`airbrush`) or colour mask, never a colour mask with `flipflake`. A
/// `.mat` naming `airbrush` (the MAN TGX Euro 6 cab) still shows colour-mask
/// jobs as colour masks. `alternate_uvset` picks the mesh, not a flag
/// ([`Paint::alternate_uvset`]). Texture flags are set when the textures are
/// bound.
pub fn paint_uniform(surface: &PaintSurface, paint: &Paint<'_>) -> PaintUniform {
    let base = paint.base_color();
    let mut uniform = PaintUniform {
        base: rgb_w(base, 1.0),
        mask_r: rgb_w(base, 1.0),
        mask_g: rgb_w(base, 1.0),
        mask_b: rgb_w(base, 1.0),
        specular: rgb_w(surface.specular, surface.shininess),
        shading: Vec4::new(
            surface.fresnel,
            FLAKE_STRENGTH,
            surface.additional_ambient,
            REFLECTION_MODULATOR,
        ),
        ..default()
    };
    let Paint::Job(job) = paint else {
        return uniform;
    };
    let flipflake = job.flipflake;
    // The game never combines colormask with flipflake: a metallic job's
    // mask is used only as a livery.
    let mask = job.mask.as_ref().filter(|_| job.airbrush || !flipflake);
    if let Some(mask) = mask {
        let airbrush = job.airbrush;
        uniform.flags |= if airbrush { AIRBRUSH } else { COLORMASK };
        let [r, g, b] = job.mask_colors.map(|c| rgb_w(c, 1.0));
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
        let flip = job.flip.strength.clamp(0.0, MAX_FLIP_WEIGHT);
        uniform.flip = rgb_w(job.flip.color, flip);
        let flake = &job.flake;
        let density = flake.density.clamp(0.0, MAX_FLAKE_DENSITY);
        uniform.flake = rgb_w(flake.color, density);
        let vratio = if flake.vratio > 0.0 {
            flake.vratio
        } else {
            ZERO_VRATIO
        };
        uniform.flake_setup = Vec4::new(
            flake.uvscale,
            flake.shininess,
            flake.clearcoat_rolloff,
            vratio,
        );
    }
    uniform
}

/// The textures a paint binds besides the `.mat`'s base texture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaintBinding {
    Mask,
    FlakeNoise,
    Reflection,
}

impl TruckPaint {
    /// Binds a loaded texture and sets its flag.
    pub fn bind(&mut self, binding: PaintBinding, image: Handle<Image>) {
        match binding {
            PaintBinding::Mask => {
                self.mask = Some(image);
                self.paint.flags |= MASK_TEXTURE;
            }
            PaintBinding::FlakeNoise => {
                self.flake_noise = Some(image);
                self.paint.flags |= FLAKE_TEXTURE;
            }
            PaintBinding::Reflection => {
                self.reflection = Some(image);
                self.paint.flags |= REFLECTION_TEXTURE;
            }
        }
    }
}

/// Paint materials, shared by every piece with the same `.mat` and paint.
#[derive(Resource, Default)]
pub struct PaintMaterials {
    materials: HashMap<(AssetId<StandardMaterial>, String), Handle<TruckPaintMaterial>>,
}

impl PaintMaterials {
    /// The material for `base` (the `.mat`'s stand-in) under `paint`.
    pub fn get(
        &mut self,
        cx: &mut MaterialContext<'_>,
        materials: &mut Assets<TruckPaintMaterial>,
        base: &Handle<StandardMaterial>,
        surface: &PaintSurface,
        paint: &Paint<'_>,
    ) -> Handle<TruckPaintMaterial> {
        let key = (base.id(), paint.key());
        if let Some(handle) = self.materials.get(&key) {
            return handle.clone();
        }
        let uniform = paint_uniform(surface, paint);
        let handle = materials.add(ExtendedMaterial {
            base: cx.standard.get(base).cloned().unwrap_or_default(),
            extension: TruckPaint {
                paint: uniform,
                light: cx.light.clone(),
                ..default()
            },
        });
        let mut request = |path: &str, binding| {
            cx.textures
                .request(cx.game, path, Target::Paint(handle.clone(), binding));
        };
        if let Some(cube) = &surface.reflection {
            request(cube, PaintBinding::Reflection);
        }
        if let Paint::Job(job) = paint {
            if let Some(Mask::Texture(path)) = &job.mask {
                if uniform.flags & (COLORMASK | AIRBRUSH) != 0 {
                    request(path, PaintBinding::Mask);
                }
            }
            if uniform.flags & FLIPFLAKE != 0 {
                request(&job.flake.noise, PaintBinding::FlakeNoise);
            }
        }
        self.materials.insert(key, handle.clone());
        handle
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specsheet_core::AccessoryId;
    use specsheet_data::paint::{FlakePaint, FlipPaint};

    fn surface(_effect: &str) -> PaintSurface {
        PaintSurface {
            specular: [0.5; 3],
            shininess: 60.0,
            fresnel: 0.2,
            additional_ambient: 0.0,
            reflection: None,
        }
    }

    fn job(mask: Option<Mask>) -> PiecePaint {
        PiecePaint {
            paint_job: AccessoryId::from("stripes.t.paint_job"),
            part: None,
            base_color: [0.1, 0.2, 0.3],
            mask,
            mask_colors: [[1.0, 0.0, 0.0], [0.1, 0.2, 0.3], [0.0, 0.0, 1.0]],
            flake: FlakePaint {
                color: [1.0, 1.0, 1.0],
                density: 2.0,
                shininess: 60.0,
                clearcoat_rolloff: 3.0,
                uvscale: 8.0,
                vratio: 0.0,
                noise: "/material/custom/flake_noise.tobj".to_owned(),
            },
            flip: FlipPaint {
                color: [0.9, 0.8, 0.7],
                strength: 2.7,
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
        assert!(!Paint::Color(None).alternate_uvset());
    }

    #[test]
    fn a_mask_becomes_a_colour_mask_or_a_livery() {
        let paint = job(Some(Mask::Texture("/m.tobj".to_owned())));
        let u = paint_uniform(&surface("truckpaint"), &Paint::Job(&paint));
        assert_eq!(u.flags, COLORMASK);
        assert_eq!(u.mask_r, Vec4::new(1.0, 0.0, 0.0, 1.0));
        assert_eq!(u.mask_g, Vec4::new(0.1, 0.2, 0.3, 1.0));
        let livery = PiecePaint {
            alternate_uvset: true,
            ..paint.clone()
        };
        let u = paint_uniform(&surface("truckpaint"), &Paint::Job(&livery));
        assert_eq!(u.flags, COLORMASK);
        assert!(Paint::Job(&livery).alternate_uvset());
        let airbrush = PiecePaint {
            airbrush: true,
            ..paint.clone()
        };
        let u = paint_uniform(&surface("truckpaint"), &Paint::Job(&airbrush));
        assert_eq!(u.flags, AIRBRUSH);
    }

    #[test]
    fn the_paint_job_not_the_mat_decides_the_variant() {
        // The MAN TGX Euro 6 cab's .mat names `airbrush`, yet Driving
        // Academy is a colour-mask job: drawn as a livery, its channel
        // weights showed as red and green stripes.
        let academy = job(Some(Mask::Texture("/m.tobj".to_owned())));
        let u = paint_uniform(
            &surface("truckpaint.tsnmapuv.airbrush.flipflake.altuv"),
            &Paint::Job(&academy),
        );
        assert_eq!(u.flags, COLORMASK);
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
        assert_eq!(u.flake_setup, Vec4::new(8.0, 60.0, 3.0, ZERO_VRATIO));
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
            additional_ambient: None,
        };
        let s = PaintSurface::of(&info).expect("paint");
        assert_eq!((s.fresnel, s.shininess), (0.3, DEFAULT_SHININESS));
        info.paintable = false;
        assert_eq!(PaintSurface::of(&info), None);
    }
}
