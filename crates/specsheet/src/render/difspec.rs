//! The `eut2.dif.spec` effect family as a Bevy material, from the spec
//! `docs/app/shader-specs/dif-spec.md`: diffuse with the base alpha as the
//! specular mask, unnormalised Blinn-Phong, and for `add.env` the `.mat`'s
//! own reflection cube with the linear fresnel, lit by the game's forward
//! model (`game_lighting.wgsl`). Normal maps (`tsnmap…`) and `oclu` are not
//! drawn yet.

use std::collections::HashMap;

use bevy::asset::embedded_asset;
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::render::storage::ShaderBuffer;
use bevy::shader::ShaderRef;
use specsheet_data::render::{BlendMode, MaterialInfo};

use super::textures::Target;
use super::MaterialContext;

/// A `dif.spec` piece's material: the `.mat`'s stand-in (base texture,
/// diffuse, vertex colour, blending) plus the family's own inputs.
pub type DifSpecMaterial = ExtendedMaterial<StandardMaterial, DifSpec>;

const SHADER: &str = "embedded://specsheet/render/difspec.wgsl";

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

/// What a `dif.spec` `.mat` gives its material.
#[derive(Clone, Debug, PartialEq)]
pub struct DifSpecSurface {
    uniform: DifSpecUniform,
    reflection: Option<String>,
}

impl DifSpecSurface {
    /// The `.mat`'s reflection cube, for `add.env` effects.
    pub fn reflection(&self) -> Option<&str> {
        self.reflection.as_deref()
    }

    /// `None` for materials outside the family, and for those this material
    /// does not draw yet: paint, self-lit, additive and `mult` effects.
    pub fn of(info: &MaterialInfo) -> Option<Self> {
        let name = info.effect.trim_end_matches(".rfx");
        let tokens: Vec<&str> = name.split('.').collect();
        let has = |t: &str| tokens.contains(&t);
        let family = tokens.get(1..3) == Some(&["dif", "spec"][..]);
        if !family
            || info.paintable
            || info.emissive
            || has("mult")
            || matches!(info.blend, BlendMode::Additive | BlendMode::Hidden)
        {
            return None;
        }
        let env = has("env");
        let mut flags = 0;
        if has("weight") {
            flags |= WEIGHT;
        }
        if env {
            flags |= ENV;
        }
        if has("nofresnel") {
            flags |= NOFRESNEL;
        }
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
        })
    }
}

/// `dif.spec` materials, one per `.mat` stand-in.
#[derive(Resource, Default)]
pub struct DifSpecMaterials {
    materials: HashMap<AssetId<StandardMaterial>, Handle<DifSpecMaterial>>,
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
        if let Some(handle) = self.materials.get(&base.id()) {
            return handle.clone();
        }
        let mut standard = cx.standard.get(base).cloned().unwrap_or_default();
        if let AlphaMode::Mask(_) = standard.alpha_mode {
            standard.alpha_mode = AlphaMode::Mask(ALPHA_CUTOFF);
        }
        let handle = materials.add(ExtendedMaterial {
            base: standard,
            extension: DifSpec {
                surface: surface.uniform,
                light: cx.light.clone(),
                reflection: None,
            },
        });
        if let Some(cube) = &surface.reflection {
            cx.textures
                .request(cx.game, cube, Target::DifSpec(handle.clone()));
        }
        self.materials.insert(base.id(), handle.clone());
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
            "eut2.dif.spec.mult.dif.spec.rfx",
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
}
