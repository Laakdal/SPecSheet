//! Vehicle light flares (F19, `docs/app/shader-specs/flare.md`): the game's
//! glow model at each light locator whose light is on, turned to the camera
//! and sized on the CPU as the engine does.

use bevy::prelude::*;
use specsheet_data::flare::VehicleLight;
use specsheet_data::scene::Scene;
use specsheet_render::assets::{GpuModel, ModelKey};

use crate::studio::{FrameSet, Studio};
use crate::viewer::ModelCache;

pub struct FlarePlugin;

impl Plugin for FlarePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FlareMaterials>().add_systems(
            Update,
            (fill_flares, switch_flares, face_camera)
                .chain()
                .in_set(FrameSet::View),
        );
    }
}

/// Flare brightness for the material's `luminance_output` of 10000 nits:
/// `ML.a = 10000 · p · 0.001` (flare.md §3; the per-draw object `o` is ours,
/// 1).
const LUMINANCE_PER_PRE_EXPOSURE: f32 = 10.0;
/// The lights a scene with `vehicle_lights_on` turns on. Ours (flare.md §5):
/// its light mask's numbering is Unknown.
const LIGHTS_ON: [&str; 2] = ["parking", "low_beam"];
/// The glow quad's size when a unit gives none.
const DEFAULT_SCALE: f32 = 1.0;

/// A light locator's flare, placed at the locator.
#[derive(Component)]
pub struct Flare {
    light: VehicleLight,
    model: ModelKey,
    filled: bool,
}

/// The camera-facing glow quad under a [`Flare`].
#[derive(Component)]
struct FlareQuad;

/// Spawns a flare under `parent` for every light locator of `model` whose
/// unit has a glow model.
pub fn spawn_flares(commands: &mut Commands, studio: &Studio, parent: Entity, model: &GpuModel) {
    let Some(game) = studio.game.as_ref() else {
        return;
    };
    for locator in &model.lights {
        let Some(light) = game.vehicle_light(&locator.hookup) else {
            continue;
        };
        let Some(path) = light.model.clone() else {
            continue;
        };
        commands.spawn((
            Flare {
                light: light.clone(),
                model: ModelKey {
                    path,
                    look: None,
                    variant: None,
                    parts: Vec::new(),
                },
                filled: false,
            },
            Transform::from_translation(Vec3::from(locator.position)),
            Visibility::Hidden,
            ChildOf(parent),
        ));
    }
}

/// The flare materials: each glow model's stand-in, scaled by the scene's
/// luminosity.
#[derive(Resource, Default)]
struct FlareMaterials {
    /// (stand-in, scaled copy).
    materials: Vec<(Handle<StandardMaterial>, Handle<StandardMaterial>)>,
    /// The luminosity the copies carry.
    luminosity: f32,
}

impl FlareMaterials {
    fn scaled(
        &mut self,
        assets: &mut Assets<StandardMaterial>,
        stand_in: &Handle<StandardMaterial>,
    ) -> Handle<StandardMaterial> {
        if let Some((_, scaled)) = self.materials.iter().find(|(s, _)| s == stand_in) {
            return scaled.clone();
        }
        let mut material = assets.get(stand_in).cloned().unwrap_or_default();
        material.base_color = scale(material.base_color, self.luminosity);
        let scaled = assets.add(material);
        self.materials.push((stand_in.clone(), scaled.clone()));
        scaled
    }

    /// Rescales every copy when the scene's luminosity changes.
    fn set_luminosity(&mut self, assets: &mut Assets<StandardMaterial>, luminosity: f32) {
        if self.luminosity == luminosity {
            return;
        }
        self.luminosity = luminosity;
        for (stand_in, scaled) in &self.materials {
            let base = assets.get(stand_in).map(|m| m.base_color);
            if let (Some(base), Some(mut material)) = (base, assets.get_mut(scaled)) {
                material.base_color = scale(base, luminosity);
            }
        }
    }
}

/// `rgb · k`, alpha kept.
fn scale(colour: Color, k: f32) -> Color {
    let c = colour.to_linear();
    Color::linear_rgba(c.red * k, c.green * k, c.blue * k, c.alpha)
}

fn current_scene(studio: &Studio) -> Option<&Scene> {
    studio
        .scene
        .and_then(|i| studio.scenes.get(i))
        .map(|s| &s.scene)
}

/// Spawns each flare's glow model once it has loaded.
fn fill_flares(
    mut commands: Commands,
    studio: Res<Studio>,
    mut cache: ResMut<ModelCache>,
    mut flares: Query<(Entity, &mut Flare)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut flare_materials: ResMut<FlareMaterials>,
) {
    let p = current_scene(&studio)
        .and_then(Scene::limited_pre_exposure)
        .unwrap_or(1.0);
    flare_materials.set_luminosity(&mut materials, LUMINANCE_PER_PRE_EXPOSURE * p);
    for (entity, mut flare) in &mut flares {
        if flare.filled {
            continue;
        }
        let Some(model) = cache.get(&studio, &flare.model) else {
            continue;
        };
        flare.filled = true;
        let quad = commands
            .spawn((
                FlareQuad,
                Transform::default(),
                Visibility::Inherited,
                ChildOf(entity),
            ))
            .id();
        for (mesh, stand_in) in &model.pieces {
            let material = flare_materials.scaled(&mut materials, stand_in);
            commands.spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material),
                bevy::light::NotShadowCaster,
                ChildOf(quad),
            ));
        }
    }
}

/// Shows the flares of the lights the scene turns on.
fn switch_flares(studio: Res<Studio>, mut flares: Query<(&Flare, &mut Visibility)>) {
    let lights_on = current_scene(&studio).is_some_and(|s| s.vehicle_lights_on);
    for (flare, mut visibility) in &mut flares {
        let on = lights_on
            && flare
                .light
                .light_type
                .as_deref()
                .is_some_and(|t| LIGHTS_ON.contains(&t));
        let wanted = if on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
}

/// The glow quad's size at `distance` from the camera (flare.md §4,
/// Hypothesis): `default_scale`, growing to `default_scale × scale_factor`
/// between the scaling distances.
fn flare_size(light: &VehicleLight, distance: f32) -> f32 {
    let size = light.default_scale.unwrap_or(DEFAULT_SCALE);
    match (light.scale_factor, light.scaling_distances) {
        (Some(factor), Some([start, end])) if end > start => {
            let t = ((distance - start) / (end - start)).clamp(0.0, 1.0);
            size * (1.0 + (factor - 1.0) * t)
        }
        _ => size,
    }
}

/// Turns every glow quad to the camera, sizes it, and moves it the unit's
/// `visual_offset` towards the camera, out of the lamp housing. Ours: the
/// spec's reading is along the locator's forward axis (flare.md §4,
/// Hypothesis), which puts a headlight's glow 0.6 m in front of the lamp
/// and beside the truck in a three-quarter view.
fn face_camera(
    camera: Single<&GlobalTransform, With<Camera3d>>,
    flares: Query<(&Flare, &GlobalTransform, &Children)>,
    mut quads: Query<&mut Transform, With<FlareQuad>>,
) {
    let eye = camera.translation();
    for (flare, global, children) in &flares {
        let (parent_scale, parent_rotation, position) = global.to_scale_rotation_translation();
        let to_eye = eye - position;
        let distance = to_eye.length();
        if distance < 1e-3 {
            continue;
        }
        // The quad lies in its xy plane; its z axis points at the eye.
        let facing = Transform::default().looking_to(-to_eye, Vec3::Y).rotation;
        let size = flare_size(&flare.light, distance);
        let scale = parent_scale.max(Vec3::splat(1e-3));
        let offset = to_eye / distance * flare.light.visual_offset.unwrap_or(0.0).min(distance);
        for child in children {
            if let Ok(mut transform) = quads.get_mut(*child) {
                transform.rotation = parent_rotation.inverse() * facing;
                transform.translation = parent_rotation.inverse() * offset / scale;
                transform.scale = Vec3::splat(size) / scale;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specsheet_data::flare::LightKind;

    fn light(scale: Option<f32>, factor: Option<f32>, distances: Option<[f32; 2]>) -> VehicleLight {
        VehicleLight {
            name: "flare.vehicle.headl".to_owned(),
            kind: LightKind::Flare,
            light_type: Some("low_beam".to_owned()),
            model: None,
            dir_type: None,
            inner_angle: None,
            default_scale: scale,
            scale_factor: factor,
            scaling_distances: distances,
            visual_offset: None,
        }
    }

    #[test]
    fn a_flare_grows_between_its_scaling_distances() {
        // `flare.vehicle.headl`: 1.53 m, ×3 between 0 and 60 m.
        let headlight = light(Some(1.53), Some(3.0), Some([0.0, 60.0]));
        assert_eq!(flare_size(&headlight, 0.0), 1.53);
        assert!((flare_size(&headlight, 30.0) - 3.06).abs() < 1e-5);
        assert!((flare_size(&headlight, 100.0) - 4.59).abs() < 1e-5);
        assert_eq!(flare_size(&light(Some(0.405), None, None), 50.0), 0.405);
        assert_eq!(flare_size(&light(None, None, None), 5.0), DEFAULT_SCALE);
    }
}
