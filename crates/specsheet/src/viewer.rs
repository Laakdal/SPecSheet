//! The 3D view: the assembled truck, the game's tuning scene (finding F7)
//! and the orbit camera. Models load on the async compute pool and are
//! shared between instances; placements are diffed so unchanged parts stay.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::camera::Exposure;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::light::{EnvironmentMapLight, GeneratedEnvironmentMapLight};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use bevy::render::renderer::RenderDevice;
use bevy::render::settings::WgpuFeatures;
use bevy::render::storage::ShaderBuffer;
use bevy::tasks::{futures::check_ready, AsyncComputeTaskPool, Task};
use bevy::window::PrimaryWindow;
use specsheet_assembly::Placement;
use specsheet_data::render::{load_cube_texture, CubeTextureData};
use specsheet_data::scene::{OrbitSpec, Scene};

use crate::studio::{FrameSet, Studio};
use specsheet_render::assets::{self, GamePiece, GpuModel, ModelKey, ModelPayload, Stores};
use specsheet_render::{
    DifSpecMaterial, DifSpecMaterials, GameLight, GameLighting, GameTextures, MaterialContext,
    Paint, PaintMaterials, TruckPaintMaterial,
};

/// The truck, the scene, lights and the orbit camera.
pub struct ViewerPlugin;

impl Plugin for ViewerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ModelCache>()
            .init_resource::<ReflectionCube>()
            .init_resource::<TruckInstances>()
            .init_resource::<SceneState>()
            .init_resource::<ViewportInput>()
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (
                    finish_models,
                    apply_scene,
                    update_reflection,
                    sync_truck,
                    frame_truck,
                    orbit_camera,
                    apply_view_mode,
                )
                    .chain()
                    .in_set(FrameSet::View),
            );
    }
}

/// Parent of every truck model; carries the scene's truck pose.
#[derive(Component)]
pub struct TruckRoot;

/// Parent of the scene (garage) model.
#[derive(Component)]
pub struct SceneRoot;

/// The floor shown while no scene model is loaded.
#[derive(Component)]
pub struct FallbackFloor;

#[derive(Component)]
pub struct Sun;

/// Orbit camera state, in the spirit of three.js `OrbitControls`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Orbit {
    pub target: Vec3,
    /// Angle around +y, 0 looking from +z.
    pub yaw: f32,
    /// Elevation above the horizontal plane.
    pub pitch: f32,
    pub distance: f32,
    pub max_distance: f32,
    pub min_pitch: f32,
    pub max_pitch: f32,
    /// Box the target may be panned within.
    pub target_min: Vec3,
    pub target_max: Vec3,
}

/// How far a panned target may move from a scene camera's aim when the
/// scene sets no bounds.
const DEFAULT_PAN_REACH: f32 = 10.0;

const MIN_DISTANCE: f32 = 2.5;
const MAX_DISTANCE: f32 = 25.0;
/// Keeps the camera just above the floor (three.js `maxPolarAngle` 0.495π).
const MIN_PITCH: f32 = 0.005 * std::f32::consts::PI;
const MAX_PITCH: f32 = 0.5 * std::f32::consts::PI - 0.01;
const ROTATE_SPEED: f32 = 0.008;

impl Orbit {
    pub fn from_pose(position: Vec3, target: Vec3) -> Self {
        let offset = position - target;
        let distance = offset.length().max(MIN_DISTANCE);
        let pitch = (offset.y / distance).clamp(-1.0, 1.0).asin();
        let orbit = Self {
            target,
            yaw: offset.x.atan2(offset.z),
            pitch,
            distance,
            max_distance: MAX_DISTANCE.max(distance),
            min_pitch: MIN_PITCH,
            max_pitch: MAX_PITCH,
            target_min: target - Vec3::splat(DEFAULT_PAN_REACH),
            target_max: target + Vec3::splat(DEFAULT_PAN_REACH),
        };
        orbit.clamped()
    }

    /// The truck configuration screen's own camera. The game's elevation is
    /// negative above the aim, so pitch is its negation, and its elevation
    /// limits become the pitch range (Hypothesis, see `OrbitSpec::position`).
    /// Non-finite values (the data layer already filters them) fall back
    /// to the viewer defaults rather than reach `clamp`, which panics on NaN.
    pub fn from_spec(spec: &OrbitSpec) -> Self {
        let finite = |v: f32, default: f32| if v.is_finite() { v } else { default };
        let distance = finite(spec.zoom * spec.relative_bounds[0], MIN_DISTANCE).max(MIN_DISTANCE);
        let [low, high] = spec.elevation_limit;
        let (min_pitch, max_pitch) = if low.is_finite() && high.is_finite() {
            ((-high).min(-low), (-high).max(-low))
        } else {
            (MIN_PITCH, MAX_PITCH)
        };
        let target = Vec3::from(spec.aim);
        let target = if target.is_finite() {
            target
        } else {
            DEFAULT_TARGET
        };
        // The target stays within `camera_relative_bounds` around the aim,
        // and its height within `camera_aim_y_bounds` (Hypothesis).
        let reach = Vec3::from(spec.relative_bounds);
        let reach = if reach.is_finite() {
            reach.abs()
        } else {
            Vec3::splat(DEFAULT_PAN_REACH)
        };
        let [y_low, y_high] = spec.aim_y_bounds;
        let (y_min, y_max) = if y_low.is_finite() && y_high.is_finite() {
            (y_low.min(y_high), y_low.max(y_high))
        } else {
            (target.y - reach.y, target.y + reach.y)
        };
        let orbit = Self {
            target_min: Vec3::new(target.x - reach.x, y_min.min(target.y), target.z - reach.z),
            target_max: Vec3::new(target.x + reach.x, y_max.max(target.y), target.z + reach.z),
            target,
            yaw: finite(spec.azimuth, 0.0),
            pitch: finite(-spec.elevation, min_pitch),
            distance,
            max_distance: MAX_DISTANCE.max(distance),
            min_pitch,
            max_pitch,
        };
        orbit.clamped()
    }

    fn clamped(mut self) -> Self {
        self.target = self.target.clamp(self.target_min, self.target_max);
        self.pitch = self.pitch.clamp(self.min_pitch, self.max_pitch);
        self.distance = self.distance.clamp(MIN_DISTANCE, self.max_distance);
        self
    }

    pub fn position(&self) -> Vec3 {
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
        self.target + Vec3::new(cos_pitch * sin_yaw, sin_pitch, cos_pitch * cos_yaw) * self.distance
    }

    pub fn transform(&self) -> Transform {
        Transform::from_translation(self.position()).looking_at(self.target, Vec3::Y)
    }
}

/// Lighting before a game scene applies, and for pieces a scene omits.
/// SCS trucks face -z; the default view is a front three-quarter one.
const DEFAULT_CAMERA: Vec3 = Vec3::new(-6.5, 3.2, -8.5);
const DEFAULT_TARGET: Vec3 = Vec3::new(0.0, 1.5, 0.0);
const DEFAULT_FOV_DEGREES: f32 = 40.0;
const DEFAULT_SUN_DIRECTION: Vec3 = Vec3::new(6.0, -10.0, 8.0);
const BACKGROUND: Color = Color::srgb(0.082, 0.094, 0.114);

/// Lux per unit of the game's `light_diffuse`, and cd/m² per unit of
/// `light_ambient` and `light_environment`, at `Exposure::BLENDER`. These
/// reproduce the three.js viewer's balance; the game's own light units are
/// Unknown.
const SUN_LUX: f32 = 1000.0;
const AMBIENT_NITS: f32 = 1500.0;
const ENVIRONMENT_NITS: f32 = 500.0;
const DEFAULT_SUN: f32 = 1.6;
/// three.js used 0xdde6ff at 0.6 against 1.5 for scenes, folded into the colour.
const DEFAULT_AMBIENT: [f32; 3] = [0.29, 0.32, 0.4];
/// The three.js viewer capped the environment at twice its default.
const MAX_ENVIRONMENT: f32 = 4.0;
const DEFAULT_ENVIRONMENT: f32 = 2.0;

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut lighting: ResMut<GameLighting>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
) {
    lighting.set(&mut buffers, game_light(None));
    let orbit = Orbit::from_pose(DEFAULT_CAMERA, DEFAULT_TARGET);
    commands.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: DEFAULT_FOV_DEGREES.to_radians(),
            near: 0.1,
            far: 500.0,
            ..default()
        }),
        Camera {
            clear_color: ClearColorConfig::Custom(BACKGROUND),
            ..default()
        },
        Tonemapping::AcesFitted,
        Exposure::BLENDER,
        environment(&mut images, DEFAULT_ENVIRONMENT),
        orbit.transform(),
        orbit,
    ));
    commands.spawn((
        Sun,
        DirectionalLight {
            illuminance: DEFAULT_SUN * SUN_LUX,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::default().looking_to(DEFAULT_SUN_DIRECTION, Vec3::Y),
    ));
    commands.insert_resource(GlobalAmbientLight {
        color: Color::linear_rgb(DEFAULT_AMBIENT[0], DEFAULT_AMBIENT[1], DEFAULT_AMBIENT[2]),
        brightness: AMBIENT_NITS,
        ..default()
    });
    commands.spawn((
        FallbackFloor,
        Mesh3d(meshes.add(Circle::new(14.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.137, 0.153, 0.18),
            perceptual_roughness: 0.9,
            ..default()
        })),
        Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
    ));
    commands.spawn((SceneRoot, Transform::default(), Visibility::default()));
    commands.spawn((TruckRoot, Transform::default(), Visibility::default()));
}

/// A neutral studio gradient, used until the game's reflection cube is
/// loaded (`light_environment` scales it).
fn environment(images: &mut Assets<Image>, strength: f32) -> EnvironmentMapLight {
    let mut light = EnvironmentMapLight::hemispherical_gradient(
        images,
        Color::linear_rgb(1.0, 1.0, 1.0),
        Color::linear_rgb(0.45, 0.45, 0.45),
        Color::linear_rgb(0.12, 0.12, 0.12),
    );
    light.intensity = strength * ENVIRONMENT_NITS;
    light
}

enum CubeState {
    Loading(Task<Result<CubeTextureData, String>>),
    /// The uploaded cube and the factor that keeps the scene's exposure.
    Ready(Handle<Image>, f32),
    Failed,
}

/// Approximate mean luminance of the gradient in `environment()`: the
/// intensities were tuned against it.
const GRADIENT_MEAN_LUMINANCE: f32 = 0.5;

/// Factor that gives a cube of mean luminance `mean` the gradient's overall
/// brightness, limited so an odd cube cannot blow out or black out the view.
fn exposure_balance(mean: Option<f32>) -> f32 {
    mean.filter(|m| *m > 0.0)
        .map_or(1.0, |m| (GRADIENT_MEAN_LUMINANCE / m).clamp(0.25, 4.0))
}

/// The game's reflection cube as the camera's environment map, in place of
/// the gradient (`docs/app/materials.md`). The cube is the one most
/// reflective materials of the truck's models name in `texture_reflection`,
/// e.g. `/material/environment/vehicle_reflection.tobj`.
#[derive(Resource)]
pub struct ReflectionCube {
    cubes: HashMap<String, CubeState>,
    /// The most used cube among the loaded models.
    wanted: Option<String>,
    /// The cube on the camera, with its exposure factor.
    active: Option<(String, f32)>,
    /// (truck generation, model-cache generation) when `wanted` was last
    /// counted.
    counted: (u64, u64),
    /// The scene's `light_environment`, capped.
    strength: f32,
}

impl Default for ReflectionCube {
    fn default() -> Self {
        Self {
            cubes: HashMap::new(),
            wanted: None,
            active: None,
            counted: (0, 0),
            strength: DEFAULT_ENVIRONMENT,
        }
    }
}

/// The most used path, ties broken by name so the choice is stable.
fn most_used<'a>(paths: impl Iterator<Item = &'a String>) -> Option<String> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for path in paths {
        *counts.entry(path).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(a.0)))
        .map(|(path, _)| path.to_owned())
}

/// Picks, loads and installs the reflection cube. Loading runs on the async
/// compute pool; a cube that fails validation leaves the gradient in place.
fn update_reflection(
    mut commands: Commands,
    studio: Res<Studio>,
    mut cache: ResMut<ModelCache>,
    mut reflection: ResMut<ReflectionCube>,
    mut images: ResMut<Assets<Image>>,
    device: Option<Res<RenderDevice>>,
    camera: Single<Entity, With<Camera3d>>,
) {
    let reflection = &mut *reflection;
    let generations = (studio.truck_generation, cache.generation);
    if reflection.counted != generations {
        // Decide once every truck model has finished loading, so the choice
        // does not flip while parts arrive.
        let models: Option<Vec<Option<Arc<GpuModel>>>> = studio
            .placements
            .iter()
            .map(
                |placement| match cache.models.get(&placement_key(placement)) {
                    Some(ModelState::Ready(model)) => Some(Some(Arc::clone(model))),
                    Some(ModelState::Failed) => Some(None),
                    _ => None,
                },
            )
            .collect();
        if let Some(models) = models {
            reflection.counted = generations;
            reflection.wanted = most_used(
                models
                    .iter()
                    .flatten()
                    .flat_map(|model| model.reflections.iter()),
            );
        }
    }
    if let (Some(path), Some(game)) = (&reflection.wanted, &studio.game) {
        if !reflection.cubes.contains_key(path) {
            let (task_path, game) = (path.clone(), game.clone());
            let task = AsyncComputeTaskPool::get().spawn(async move {
                load_cube_texture(game.fs(), &task_path).map_err(|e| e.to_string())
            });
            reflection
                .cubes
                .insert(path.clone(), CubeState::Loading(task));
        }
    }
    let bc_supported =
        device.is_some_and(|d| d.features().contains(WgpuFeatures::TEXTURE_COMPRESSION_BC));
    for (path, state) in &mut reflection.cubes {
        let CubeState::Loading(task) = state else {
            continue;
        };
        let Some(result) = check_ready(task) else {
            continue;
        };
        let image = result.and_then(|data| {
            let balance = exposure_balance(assets::cube_mean_luminance(&data));
            assets::cube_image(data, bc_supported).map(|image| (image, balance))
        });
        *state = match image {
            Ok((image, balance)) => CubeState::Ready(images.add(image), balance),
            Err(error) => {
                cache.warnings.push(format!("{path}: {error}"));
                CubeState::Failed
            }
        };
    }
    let Some(path) = reflection.wanted.clone() else {
        return;
    };
    if reflection
        .active
        .as_ref()
        .is_some_and(|(active, _)| *active == path)
    {
        return;
    }
    let Some(CubeState::Ready(handle, balance)) = reflection.cubes.get(&path) else {
        return;
    };
    let balance = *balance;
    // Bevy only generates the filtered maps for a camera without an
    // `EnvironmentMapLight`, then adds one itself.
    commands
        .entity(*camera)
        .remove::<EnvironmentMapLight>()
        .insert(GeneratedEnvironmentMapLight {
            environment_map: handle.clone(),
            intensity: reflection.strength * ENVIRONMENT_NITS * balance,
            ..default()
        });
    info!("environment map: {path} (exposure x{balance:.2})");
    reflection.active = Some((path, balance));
}

enum ModelState {
    Loading(Task<Result<ModelPayload, String>>),
    Ready(Arc<GpuModel>),
    Failed,
}

/// Every model requested so far, by key, and the converted textures.
#[derive(Resource, Default)]
pub struct ModelCache {
    models: HashMap<ModelKey, ModelState>,
    textures: HashMap<String, Option<Handle<Image>>>,
    /// Bumped whenever a model finishes loading.
    generation: u64,
    /// Load problems, newest last.
    pub warnings: Vec<String>,
}

impl ModelCache {
    /// The model if it is ready; starts loading it otherwise.
    fn get(&mut self, studio: &Studio, key: &ModelKey) -> Option<Arc<GpuModel>> {
        match self.models.get(key) {
            Some(ModelState::Ready(model)) => Some(Arc::clone(model)),
            Some(_) => None,
            None => {
                let game = studio.game.clone()?;
                let task_key = key.clone();
                let task = AsyncComputeTaskPool::get()
                    .spawn(async move { assets::load_model(&game, &task_key) });
                self.models.insert(key.clone(), ModelState::Loading(task));
                None
            }
        }
    }

    fn failed(&self, key: &ModelKey) -> bool {
        matches!(self.models.get(key), Some(ModelState::Failed))
    }

    pub fn loading(&self) -> bool {
        self.models
            .values()
            .any(|m| matches!(m, ModelState::Loading(_)))
    }
}

/// Uploads models whose loading finished.
fn finish_models(
    mut cache: ResMut<ModelCache>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    device: Option<Res<RenderDevice>>,
) {
    let bc_supported =
        device.is_some_and(|d| d.features().contains(WgpuFeatures::TEXTURE_COMPRESSION_BC));
    let cache = &mut *cache;
    let mut finished = Vec::new();
    for (key, state) in &mut cache.models {
        if let ModelState::Loading(task) = state {
            if let Some(result) = check_ready(task) {
                finished.push((key.clone(), result));
            }
        }
    }
    for (key, result) in finished {
        let state = match result {
            Ok(payload) => {
                let mut stores = Stores {
                    meshes: &mut meshes,
                    materials: &mut materials,
                    images: &mut images,
                    textures: &mut cache.textures,
                    bc_supported,
                };
                let (model, warnings) = assets::upload(payload, &mut stores);
                cache.warnings.extend(warnings);
                ModelState::Ready(model)
            }
            Err(error) => {
                cache.warnings.push(format!("{}: {error}", key.path));
                ModelState::Failed
            }
        };
        cache.models.insert(key, state);
        cache.generation += 1;
    }
}

fn placement_key(placement: &Placement) -> ModelKey {
    ModelKey {
        path: placement.model.clone(),
        look: placement.look.clone(),
        variant: placement.variant.clone(),
        parts: Vec::new(),
    }
}

/// Instances are diffed by model, transform and paint, so a paint change
/// rebuilds exactly the parts it touches.
fn instance_key(placement: &Placement, paint: &Paint<'_>) -> String {
    let t = &placement.transform;
    format!(
        "{}|{:?}|{:?}|{:?}|{:?}|{}|{}",
        placement.model,
        placement.look,
        placement.variant,
        t.translation,
        t.rotation,
        t.scale,
        paint.key()
    )
}

/// The spawned truck and what it was built from.
#[derive(Resource, Default)]
pub struct TruckInstances {
    entities: HashMap<String, Entity>,
    /// Truck-space driver's eye: the cockpit's placement.
    pub eye: Option<Vec3>,
    /// (truck generation, model-cache generation) last synced.
    synced: (u64, u64),
    /// Truck-space bounds of the placed models.
    pub bounds: Option<(Vec3, Vec3)>,
}

/// Which view an instance belongs to.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewPart {
    Exterior,
    /// The light exterior-view interior; hidden from inside.
    InteriorShell,
    /// The full cockpit; shown only from inside.
    Cockpit,
}

/// Shows the cockpit or the exterior-view interior, by view.
fn apply_view_mode(input: Res<ViewportInput>, mut parts: Query<(&ViewPart, &mut Visibility)>) {
    for (part, mut visibility) in &mut parts {
        let shown = match part {
            ViewPart::Exterior => true,
            ViewPart::InteriorShell => !input.interior_view,
            ViewPart::Cockpit => input.interior_view,
        };
        let wanted = if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
}

/// Brings the spawned truck in line with the studio's placements.
fn sync_truck(
    mut commands: Commands,
    studio: Res<Studio>,
    mut cache: ResMut<ModelCache>,
    mut instances: ResMut<TruckInstances>,
    mut paints: PieceMaterials,
    root: Single<Entity, With<TruckRoot>>,
) {
    let generations = (studio.truck_generation, cache.generation);
    if instances.synced == generations {
        return;
    }
    instances.synced = generations;
    // Placements without a paint job take the configuration's colour.
    let fallback = studio.overview.as_ref().and_then(|o| o.paint_color);
    let paint_of = |p: &Placement| {
        studio
            .paints
            .get(&(p.slot.clone(), p.accessory.clone()))
            .map_or_else(|| Paint::Color(fallback), Paint::Job)
    };
    // The cockpit is an instance of its own, shown only in the interior
    // view; the exterior-view interior is hidden there instead.
    let cockpit = studio.cockpit.as_ref();
    let wanted: HashMap<String, (&Placement, ViewPart)> = studio
        .placements
        .iter()
        .map(|p| {
            let part = if cockpit.is_some_and(|c| c.accessory == p.accessory) {
                ViewPart::InteriorShell
            } else {
                ViewPart::Exterior
            };
            (instance_key(p, &paint_of(p)), (p, part))
        })
        .chain(cockpit.into_iter().chain(&studio.interior_parts).map(|p| {
            (
                format!("cockpit|{}", instance_key(p, &paint_of(p))),
                (p, ViewPart::Cockpit),
            )
        }))
        .collect();
    instances.eye = cockpit.map(|c| Vec3::from(c.transform.translation));
    instances.entities.retain(|key, entity| {
        let keep = wanted.contains_key(key);
        if !keep {
            commands.entity(*entity).despawn();
        }
        keep
    });
    let mut bounds: Option<(Vec3, Vec3)> = None;
    for (key, (placement, part)) in &wanted {
        let model_key = placement_key(placement);
        let Some(model) = cache.get(&studio, &model_key) else {
            continue;
        };
        let t = &placement.transform;
        let transform = Transform {
            translation: Vec3::from(t.translation),
            rotation: Quat::from_array(t.rotation),
            scale: Vec3::splat(t.scale),
        };
        let corners = [model.aabb_min, model.aabb_max];
        let outside = *part != ViewPart::Cockpit;
        for corner in (0..8).filter(|_| outside).map(|i| {
            Vec3::new(
                corners[i & 1].x,
                corners[(i >> 1) & 1].y,
                corners[(i >> 2) & 1].z,
            )
        }) {
            let p = transform.transform_point(corner);
            bounds = Some(bounds.map_or((p, p), |(lo, hi)| (lo.min(p), hi.max(p))));
        }
        if instances.entities.contains_key(key) {
            continue;
        }
        let entity = commands
            .spawn((transform, Visibility::default(), *part, ChildOf(*root)))
            .id();
        paints.spawn(&mut commands, &studio, entity, &model, &paint_of(placement));
        instances.entities.insert(key.clone(), entity);
    }
    instances.bounds = bounds;
}

/// What spawning a model's pieces needs: `truckpaint` pieces get a paint
/// material, the rest their `.mat`'s stand-in.
#[derive(bevy::ecs::system::SystemParam)]
struct PieceMaterials<'w> {
    paints: ResMut<'w, PaintMaterials>,
    dif_specs: ResMut<'w, DifSpecMaterials>,
    textures: ResMut<'w, GameTextures>,
    standard: Res<'w, Assets<StandardMaterial>>,
    paint_materials: ResMut<'w, Assets<TruckPaintMaterial>>,
    dif_spec_materials: ResMut<'w, Assets<DifSpecMaterial>>,
    lighting: ResMut<'w, GameLighting>,
    buffers: ResMut<'w, Assets<ShaderBuffer>>,
}

impl PieceMaterials<'_> {
    /// Lights every game material with `scene`'s light, or the defaults.
    fn set_light(&mut self, scene: Option<&Scene>) {
        self.lighting.set(&mut self.buffers, game_light(scene));
    }

    /// Spawns `model`'s pieces under `parent`, painted with `paint`.
    fn spawn(
        &mut self,
        commands: &mut Commands,
        studio: &Studio,
        parent: Entity,
        model: &GpuModel,
        paint: &Paint<'_>,
    ) {
        let mut cx = MaterialContext {
            game: studio.game.as_ref(),
            standard: &self.standard,
            textures: &mut self.textures,
            light: &self.lighting.buffer,
        };
        for ((mesh, material), game) in model.pieces.iter().zip(&model.game) {
            match game {
                None => {
                    commands.spawn((
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(material.clone()),
                        ChildOf(parent),
                    ));
                }
                Some(GamePiece::Paint(painted)) => {
                    let mesh = painted
                        .alternate_mesh
                        .as_ref()
                        .filter(|_| paint.alternate_uvset())
                        .unwrap_or(mesh);
                    let material = self.paints.get(
                        &mut cx,
                        &mut self.paint_materials,
                        material,
                        &painted.surface,
                        paint,
                    );
                    commands.spawn((
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(material),
                        ChildOf(parent),
                    ));
                }
                Some(GamePiece::DifSpec(surface)) => {
                    let material = self.dif_specs.get(
                        &mut cx,
                        &mut self.dif_spec_materials,
                        material,
                        surface,
                    );
                    commands.spawn((
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(material),
                        ChildOf(parent),
                    ));
                }
            }
        }
    }
}

/// The scene as last applied, and its model once spawned.
#[derive(Resource, Default)]
pub struct SceneState {
    generation: u64,
    model: Option<(ModelKey, [f32; 3])>,
    model_entity: Option<Entity>,
    /// True while the scene supplies the camera: the target is not re-centred.
    pub scene_camera: bool,
}

/// Applies the selected game scene: camera, lights, fog, truck pose and the
/// scene model. Each piece falls back on its own.
fn apply_scene(
    mut commands: Commands,
    studio: Res<Studio>,
    mut state: ResMut<SceneState>,
    mut cache: ResMut<ModelCache>,
    mut images: ResMut<Assets<Image>>,
    mut paints: PieceMaterials,
    mut ambient: ResMut<GlobalAmbientLight>,
    camera: Single<(Entity, &mut Orbit, &mut Projection), With<Camera3d>>,
    mut sun: Single<(&mut DirectionalLight, &mut Transform), (With<Sun>, Without<TruckRoot>)>,
    mut truck: Single<&mut Transform, (With<TruckRoot>, Without<Sun>)>,
    mut floor: Single<&mut Visibility, With<FallbackFloor>>,
    scene_root: Single<Entity, With<SceneRoot>>,
    mut input: ResMut<ViewportInput>,
    mut reflection: ResMut<ReflectionCube>,
    mut environment_lights: Query<
        (
            Option<&mut EnvironmentMapLight>,
            Option<&mut GeneratedEnvironmentMapLight>,
        ),
        With<Camera3d>,
    >,
) {
    let (camera_entity, mut orbit, mut projection) = camera.into_inner();
    if state.generation != studio.scene_generation {
        state.generation = studio.scene_generation;
        let Some(scene) = studio.scene.and_then(|i| studio.scenes.get(i)) else {
            return;
        };
        let spec = scene.orbit.as_ref();
        let scene = &scene.scene;
        if let Some(pose) = &scene.camera {
            *orbit = match spec {
                Some(spec) => Orbit::from_spec(spec),
                None => Orbit::from_pose(Vec3::from(pose.position), Vec3::from(pose.aim)),
            };
            input.home = Some(*orbit);
            if let Projection::Perspective(perspective) = &mut *projection {
                perspective.fov = pose.fov.to_radians();
            }
            state.scene_camera = true;
        }
        let (sun_light, sun_transform) = &mut *sun;
        apply_lights(scene, &mut ambient, sun_light, sun_transform);
        paints.set_light(Some(scene));
        let strength = scene
            .light_environment
            .map_or(DEFAULT_ENVIRONMENT, |e| e.min(MAX_ENVIRONMENT));
        reflection.strength = strength;
        let mut camera_commands = commands.entity(camera_entity);
        if let Some((_, balance)) = reflection.active {
            // The generated map copies its intensity once, so set both.
            let intensity = strength * ENVIRONMENT_NITS * balance;
            for (light, generated) in &mut environment_lights {
                if let Some(mut light) = light {
                    light.intensity = intensity;
                }
                if let Some(mut generated) = generated {
                    generated.intensity = intensity;
                }
            }
        } else {
            camera_commands.insert(environment(&mut images, strength));
        }
        match (scene.fog_color, scene.fog_density) {
            (Some([r, g, b]), Some(density)) => camera_commands.insert(DistanceFog {
                color: Color::linear_rgb(r, g, b),
                falloff: FogFalloff::ExponentialSquared { density },
                ..default()
            }),
            _ => camera_commands.remove::<DistanceFog>(),
        };
        let [rx, ry, rz] = scene.truck_rotation.map(f32::to_radians);
        **truck = Transform {
            translation: Vec3::from(scene.truck_position),
            rotation: Quat::from_euler(EulerRot::XYZ, rx, ry, rz),
            ..default()
        };
        if let Some(entity) = state.model_entity.take() {
            commands.entity(entity).despawn();
        }
        state.model = scene.model.as_ref().map(|m| {
            let key = ModelKey {
                path: m.path.clone(),
                look: m.look.clone(),
                variant: m.variant.clone(),
                parts: m.parts.clone(),
            };
            (key, m.position)
        });
        **floor = Visibility::Inherited;
    }
    if state.model_entity.is_some() {
        return;
    }
    let Some((key, position)) = state.model.clone() else {
        return;
    };
    if cache.failed(&key) {
        state.model = None;
        return;
    }
    let Some(model) = cache.get(&studio, &key) else {
        return;
    };
    let entity = commands
        .spawn((
            Transform::from_translation(Vec3::from(position)),
            Visibility::default(),
            ChildOf(*scene_root),
        ))
        .id();
    paints.spawn(&mut commands, &studio, entity, &model, &Paint::Color(None));
    state.model_entity = Some(entity);
    **floor = Visibility::Hidden;
}

/// The game materials' light (dif-spec.md §3.3) from `scene`, with the
/// viewer's defaults where it has none.
fn game_light(scene: Option<&Scene>) -> GameLight {
    let travel = scene
        .and_then(|s| s.light_direction)
        .map_or(DEFAULT_SUN_DIRECTION, Vec3::from);
    GameLight::new(
        travel,
        scene
            .and_then(|s| s.light_diffuse)
            .unwrap_or([DEFAULT_SUN; 3]),
        scene.and_then(|s| s.light_specular),
        scene
            .and_then(|s| s.light_ambient)
            .unwrap_or(DEFAULT_AMBIENT),
        scene
            .and_then(|s| s.light_environment)
            .unwrap_or(DEFAULT_ENVIRONMENT),
    )
}

fn apply_lights(
    scene: &Scene,
    ambient: &mut GlobalAmbientLight,
    sun: &mut DirectionalLight,
    sun_transform: &mut Transform,
) {
    if let Some([r, g, b]) = scene.light_ambient {
        ambient.color = Color::linear_rgb(r, g, b);
    }
    if let Some(diffuse) = scene.light_diffuse {
        let peak = diffuse.iter().copied().fold(1e-3, f32::max);
        let [r, g, b] = diffuse.map(|c| c / peak);
        sun.color = Color::linear_rgb(r, g, b);
        sun.illuminance = peak * SUN_LUX;
    }
    if let Some(direction) = scene.light_direction {
        // The definition gives the direction the light travels.
        let direction = Vec3::from(direction);
        if direction.length_squared() > 1e-6 {
            *sun_transform = Transform::default().looking_to(direction, Vec3::Y);
        }
    }
}

/// Centres the orbit on the truck when no game scene supplies a camera.
fn frame_truck(
    state: Res<SceneState>,
    instances: Res<TruckInstances>,
    truck: Single<&GlobalTransform, With<TruckRoot>>,
    mut orbit: Single<&mut Orbit>,
) {
    if state.scene_camera {
        return;
    }
    let Some((lo, hi)) = instances.bounds else {
        return;
    };
    let center = truck.transform_point((lo + hi) / 2.0);
    if center.distance(orbit.target) > 0.5 {
        orbit.target = center;
    }
}

/// Whether the pointer may drive the camera, decided by the UI each frame.
#[derive(Resource, Default)]
pub struct ViewportInput {
    /// The pointer is over the 3D view and no marker or panel.
    pub hovered: bool,
    /// The camera sits at the driver's eye (interior tabs), set by the UI.
    pub interior_view: bool,
    /// Where the driver looks: `(yaw, pitch)`, 0 looking ahead.
    head: Vec2,
    /// Whether the last frame was the interior view.
    was_interior: bool,
    dragging: bool,
    /// Cursor position last frame, for drag deltas.
    last_cursor: Option<Vec2>,
    /// The scene's default view, restored with Home.
    home: Option<Orbit>,
}

/// The driver's initial look: ahead and slightly down at the dashboard.
const INTERIOR_LOOK: Vec2 = Vec2::new(0.0, -0.2);
/// How far the driver may turn the head.
const MAX_HEAD_YAW: f32 = 2.6;
const MAX_HEAD_PITCH: f32 = 1.2;

/// The camera at the driver's eye (`eye`, truck space) looking along
/// `head` = (yaw, pitch); trucks face −z.
fn head_transform(truck: Transform, eye: Vec3, head: Vec2) -> Transform {
    let local = Quat::from_euler(EulerRot::YXZ, head.x, head.y, 0.0);
    let position = truck.transform_point(eye);
    Transform::from_translation(position).with_rotation(truck.rotation * local)
}

/// In the interior view, a drag turns the driver's head instead.
/// Left drag rotates, right or middle drag pans, the wheel zooms, Home
/// restores the scene's view. Drags follow the cursor inside the window, so
/// a release the window never sees (outside it, or after losing focus) can
/// not leave the camera following the mouse.
fn orbit_camera(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut input: ResMut<ViewportInput>,
    instances: Res<TruckInstances>,
    truck: Single<&GlobalTransform, With<TruckRoot>>,
    camera: Single<(&mut Orbit, &mut Transform, &Projection)>,
) {
    let (mut orbit, mut transform, projection) = camera.into_inner();
    let pressed = [MouseButton::Left, MouseButton::Right, MouseButton::Middle];
    let cursor = window.cursor_position().filter(|_| window.focused);
    if buttons.any_just_pressed(pressed) && input.hovered && cursor.is_some() {
        input.dragging = true;
    }
    if !buttons.any_pressed(pressed) || cursor.is_none() {
        input.dragging = false;
    }
    let delta = match (input.dragging, cursor, input.last_cursor) {
        (true, Some(now), Some(before)) => now - before,
        _ => Vec2::ZERO,
    };
    input.last_cursor = cursor;
    let mut next = *orbit;
    if keys.just_pressed(KeyCode::Home) {
        if let Some(home) = input.home {
            next = home;
        }
    }
    if input.interior_view {
        if let Some(eye) = instances.eye {
            if !input.was_interior {
                input.head = INTERIOR_LOOK;
            }
            input.was_interior = true;
            if input.dragging {
                input.head.x -= delta.x * ROTATE_SPEED;
                input.head.y -= delta.y * ROTATE_SPEED;
            }
            input.head.x = input.head.x.clamp(-MAX_HEAD_YAW, MAX_HEAD_YAW);
            input.head.y = input.head.y.clamp(-MAX_HEAD_PITCH, MAX_HEAD_PITCH);
            *transform = head_transform(truck.compute_transform(), eye, input.head);
            return;
        }
    }
    input.was_interior = false;
    let motion = AccumulatedMouseMotion { delta };
    if input.dragging && motion.delta != Vec2::ZERO {
        if buttons.pressed(MouseButton::Left) {
            next.yaw -= motion.delta.x * ROTATE_SPEED;
            next.pitch += motion.delta.y * ROTATE_SPEED;
        } else {
            let fov = match projection {
                Projection::Perspective(p) => p.fov,
                _ => DEFAULT_FOV_DEGREES.to_radians(),
            };
            // One pixel of drag moves the target by about one pixel on screen.
            let scale = 2.0 * next.distance * (fov / 2.0).tan() / 800.0;
            let right = *transform.right();
            let up = *transform.up();
            next.target += (-motion.delta.x * right + motion.delta.y * up) * scale;
        }
    }
    if input.hovered && scroll.delta.y != 0.0 {
        let steps = match scroll.unit {
            MouseScrollUnit::Line => scroll.delta.y,
            MouseScrollUnit::Pixel => scroll.delta.y / 100.0,
        };
        next.distance *= 0.9_f32.powf(steps);
    }
    let next = next.clamped();
    if next != *orbit || transform.translation != next.position() {
        *orbit = next;
        *transform = next.transform();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_game_orbit_matches_its_default_pose_and_limits() {
        let spec = OrbitSpec {
            aim: [0.0, 2.6, -1.465],
            fov: 50.0,
            azimuth: -2.516,
            elevation: -0.068,
            zoom: 1.0,
            elevation_limit: [-0.35, 0.0],
            relative_bounds: [7.5; 3],
            aim_y_bounds: [-8.0, 10.0],
            rotation_sensitivity: 0.2,
            zoom_sensitivity: 0.2,
        };
        let orbit = Orbit::from_spec(&spec);
        assert!(orbit.position().distance(Vec3::from(spec.position())) < 1e-4);
        assert_eq!((orbit.min_pitch, orbit.max_pitch), (0.0, 0.35));
    }

    #[test]
    fn the_driver_looks_ahead_along_minus_z() {
        let truck = Transform::from_translation(Vec3::new(1.0, 0.0, 0.0));
        let eye = Vec3::new(-0.683, 2.705, -1.862);
        let camera = head_transform(truck, eye, Vec2::ZERO);
        assert!(camera.translation.distance(Vec3::new(0.317, 2.705, -1.862)) < 1e-5);
        assert!(camera.forward().distance(Vec3::NEG_Z) < 1e-5);
        // Turning left (positive yaw) looks towards −x.
        let left = head_transform(truck, eye, Vec2::new(0.5, 0.0));
        assert!(left.forward().x < 0.0);
    }

    #[test]
    fn panning_stays_within_the_scene_bounds() {
        let spec = OrbitSpec {
            aim: [0.0, 2.6, -1.465],
            fov: 50.0,
            azimuth: -2.516,
            elevation: -0.068,
            zoom: 1.0,
            elevation_limit: [-0.35, 0.0],
            relative_bounds: [7.5; 3],
            aim_y_bounds: [-8.0, 10.0],
            rotation_sensitivity: 0.2,
            zoom_sensitivity: 0.2,
        };
        let mut orbit = Orbit::from_spec(&spec);
        orbit.target = Vec3::new(1000.0, -1000.0, 1000.0);
        let orbit = orbit.clamped();
        assert_eq!(orbit.target, Vec3::new(7.5, -8.0, -1.465 + 7.5));
        let mut free = Orbit::from_pose(Vec3::new(0.0, 3.0, -8.0), Vec3::ZERO);
        free.target = Vec3::splat(1.0e6);
        assert_eq!(free.clamped().target, Vec3::splat(DEFAULT_PAN_REACH));
    }

    #[test]
    fn non_finite_game_limits_do_not_panic() {
        let spec = OrbitSpec {
            aim: [0.0, 2.6, 0.0],
            fov: 50.0,
            azimuth: 0.0,
            elevation: 0.0,
            zoom: 1.0,
            elevation_limit: [f32::NAN, f32::NAN],
            relative_bounds: [f32::INFINITY; 3],
            aim_y_bounds: [0.0, 1.0],
            rotation_sensitivity: 1.0,
            zoom_sensitivity: 1.0,
        };
        let orbit = Orbit::from_spec(&spec);
        assert_eq!((orbit.min_pitch, orbit.max_pitch), (MIN_PITCH, MAX_PITCH));
        assert!(orbit.position().is_finite());
    }

    #[test]
    fn orbit_round_trips_a_pose() {
        let orbit = Orbit::from_pose(Vec3::new(-6.5, 3.2, -8.5), Vec3::new(0.0, 1.5, 0.0));
        assert!(orbit.position().distance(Vec3::new(-6.5, 3.2, -8.5)) < 1e-4);
    }

    #[test]
    fn orbit_keeps_the_camera_above_the_floor_and_in_range() {
        let low = Orbit::from_pose(Vec3::new(0.0, -5.0, 10.0), Vec3::ZERO);
        assert!(low.pitch >= MIN_PITCH);
        let near = Orbit::from_pose(Vec3::new(0.0, 0.5, 0.5), Vec3::ZERO);
        assert_eq!(near.distance, MIN_DISTANCE);
        let far = Orbit::from_pose(Vec3::new(0.0, 10.0, 40.0), Vec3::ZERO);
        assert!(
            far.distance > MAX_DISTANCE,
            "scene cameras may start farther out"
        );
    }
}
