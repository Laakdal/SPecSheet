//! The game's truck scenes:
//! * the truck configuration screen's own scene, named by the `config_path`
//!   of the `ui_truck_scene` widget in `/ui/truck_configuration_bcg.sii`
//!   (`ui_truck_scene_config`: garage model, lights, an orbit camera and
//!   per-slot pin data; see [`tuning_scene`]);
//! * the hub ("desktop") scenes: `/def/desktop/config*.sii`
//!   (`ui_truck_scene_base_config`, finding F7) and their cinematic cameras
//!   (`/def/camera/ui/*.sii`, `cinematic_camera` / `orbit_camera` /
//!   `camera_animation_info`).

use specsheet_formats::sii::{SiiDocument, Unit, Value};

use crate::error::{GameDataError, Result};
use crate::fs::GameFs;

/// Background layout of the truck configuration screen. The executable
/// names it next to the `truck_config_hdl_u` strings (F8); the screen's
/// `screen_desc` does not list it.
const TUNING_BACKGROUND_LAYOUT: &str = "/ui/truck_configuration_bcg.sii";
const TUNING_SCENE_WIDGET: &str = "ui_truck_scene";
const TUNING_SCENE_CLASS: &str = "ui_truck_scene_config";

/// The truck configuration screen's orbit camera, in the definition's own
/// terms: angles in radians around `aim`.
#[derive(Clone, Debug, PartialEq)]
pub struct OrbitSpec {
    pub aim: [f32; 3],
    /// Vertical field of view in degrees.
    pub fov: f32,
    pub azimuth: f32,
    pub elevation: f32,
    pub zoom: f32,
    /// `(min, max)` elevation.
    pub elevation_limit: [f32; 2],
    pub relative_bounds: [f32; 3],
    pub aim_y_bounds: [f32; 2],
    pub rotation_sensitivity: f32,
    pub zoom_sensitivity: f32,
}

impl OrbitSpec {
    /// The default camera position (Hypothesis): `azimuth` turns around +y
    /// from +z towards +x, negative `elevation` raises the camera, and the
    /// distance is `zoom` times the x extent of `relative_bounds`. With the
    /// 1.60 values this is a front-left three-quarter view about 7.5 m out.
    pub fn position(&self) -> [f32; 3] {
        let distance = self.zoom * self.relative_bounds[0];
        let (sin_az, cos_az) = self.azimuth.sin_cos();
        let (sin_el, cos_el) = self.elevation.sin_cos();
        [
            self.aim[0] + distance * cos_el * sin_az,
            self.aim[1] - distance * sin_el,
            self.aim[2] + distance * cos_el * cos_az,
        ]
    }
}

/// Where and how a slot's pin is drawn (`pin_*` arrays, parallel by index).
#[derive(Clone, Debug, PartialEq)]
pub struct PinSpec {
    /// Slot name the entry applies to (Hypothesis: 60 of 65 match a truck
    /// slot directory).
    pub group: String,
    /// Pin style: `custom_acc`, or `acc_cover` / `acc_hub` / `acc_nuts`.
    pub look: String,
    pub offset: [f32; 3],
    pub dir: [f32; 3],
    /// Direction for right-hand-drive (UK) trucks.
    pub dir_uk: [f32; 3],
    pub double_sided: bool,
}

/// The truck configuration screen's scene: what the game shows behind the
/// tuning UI.
#[derive(Clone, Debug, PartialEq)]
pub struct TuningScene {
    /// Model, lights and truck pose; `camera` is the orbit's default pose.
    pub scene: Scene,
    pub orbit: Option<OrbitSpec>,
    pub pins: Vec<PinSpec>,
    /// Slots whose pins only show in the interior view.
    pub interior_only: Vec<String>,
    /// Hookup slots whose pins only show in the exterior view.
    pub hookups_exterior_only: Vec<String>,
}

/// Loads the truck configuration screen's scene through its background
/// layout's `config_path`.
pub fn tuning_scene(fs: &GameFs) -> Result<TuningScene> {
    let layout = fs.read_sii(TUNING_BACKGROUND_LAYOUT)?;
    let path = layout
        .units_of_class(TUNING_SCENE_WIDGET)
        .find_map(|unit| unit.get_str("config_path"))
        .ok_or_else(|| GameDataError::Definition {
            path: TUNING_BACKGROUND_LAYOUT.to_owned(),
            message: format!("no {TUNING_SCENE_WIDGET} with a config_path"),
        })?
        .to_owned();
    let doc = fs.read_sii(&path)?;
    let unit = doc
        .units_of_class(TUNING_SCENE_CLASS)
        .next()
        .ok_or_else(|| GameDataError::Definition {
            path: path.clone(),
            message: format!("no {TUNING_SCENE_CLASS} unit"),
        })?;
    Ok(tuning_scene_from_unit(fs, &path, unit))
}

fn tuning_scene_from_unit(fs: &GameFs, path: &str, unit: &Unit) -> TuningScene {
    let mut scene = scene_from_unit(fs, path, unit);
    // Non-finite values (SII hex floats can encode NaN and inf) fall back
    // to the defaults, like missing ones.
    let vec3 = |key: &str| unit.get(key).and_then(floats3);
    let vec2 = |key: &str| unit.get(key).and_then(floats2);
    let num = |key: &str| unit.get_f32(key).filter(|v| v.is_finite());
    let orbit = vec3("camera_aim").map(|aim| OrbitSpec {
        aim,
        fov: num("camera_fov").unwrap_or(45.0),
        azimuth: num("camera_default_azimuth").unwrap_or(0.0),
        elevation: num("camera_default_elevation").unwrap_or(0.0),
        zoom: num("camera_default_zoom").unwrap_or(1.0),
        elevation_limit: vec2("camera_elevation_limit").unwrap_or([-1.5, 1.5]),
        relative_bounds: vec3("camera_relative_bounds").unwrap_or([7.5; 3]),
        aim_y_bounds: vec2("camera_aim_y_bounds").unwrap_or([f32::MIN, f32::MAX]),
        rotation_sensitivity: num("mouse_rot_sens").unwrap_or(1.0),
        zoom_sensitivity: num("camera_zoom_sens").unwrap_or(1.0),
    });
    if let Some(orbit) = &orbit {
        scene.camera = Some(CameraPose {
            position: orbit.position(),
            aim: orbit.aim,
            fov: orbit.fov,
        });
    }
    let strings =
        |key: &str| -> Vec<String> { unit.array_str(key).into_iter().map(str::to_owned).collect() };
    let looks = strings("pin_look");
    let offsets = unit.array("pin_offset");
    let dirs = unit.array("pin_dir");
    let dirs_uk = unit.array("pin_dir_uk");
    let double_sided = unit.array("double_sided");
    // The arrays are parallel; an entry missing a field gets a neutral one.
    let pins = strings("pin_group")
        .into_iter()
        .enumerate()
        .map(|(i, group)| {
            let dir = dirs
                .get(i)
                .and_then(|v| floats3(v))
                .unwrap_or([0.0, 0.0, 1.0]);
            PinSpec {
                group,
                look: looks.get(i).cloned().unwrap_or_default(),
                offset: offsets.get(i).and_then(|v| floats3(v)).unwrap_or([0.0; 3]),
                dir,
                dir_uk: dirs_uk.get(i).and_then(|v| floats3(v)).unwrap_or(dir),
                double_sided: double_sided
                    .get(i)
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            }
        })
        .collect();
    TuningScene {
        scene,
        orbit,
        pins,
        interior_only: strings("interior_only"),
        hookups_exterior_only: strings("hookups_exterior_only"),
    }
}

const SCENE_DIR: &str = "/def/desktop";
const SCENE_CLASS: &str = "ui_truck_scene_base_config";
/// `config.sii` is the plain garage, the default scene.
const DEFAULT_SCENE_FILE: &str = "config.sii";

#[derive(Clone, Debug, PartialEq)]
pub struct CameraPose {
    pub position: [f32; 3],
    pub aim: [f32; 3],
    /// Vertical field of view in degrees.
    pub fov: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneModel {
    pub path: String,
    pub look: Option<String>,
    pub variant: Option<String>,
    pub position: [f32; 3],
    /// Parts drawn, in the game's `part_order[]`; empty means all parts.
    pub parts: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    /// Definition file, e.g. `/def/desktop/config.hangar.sii`.
    pub path: String,
    /// Localisation key or text (`@@garage_scene@@`).
    pub name: String,
    pub model: Option<SceneModel>,
    pub camera: Option<CameraPose>,
    /// Direction the light travels (not normalised).
    pub light_direction: Option<[f32; 3]>,
    pub light_ambient: Option<[f32; 3]>,
    pub light_diffuse: Option<[f32; 3]>,
    pub light_specular: Option<[f32; 3]>,
    pub light_environment: Option<f32>,
    /// Exposure the scene asks for (`pre_exposure`; only the Halloween hub
    /// sets it, to 1.0). How the engine applies it is Unknown.
    pub pre_exposure: Option<f32>,
    /// Cube that fills the engine's vehicle reflection slot in this scene
    /// (`vehicle_reflection`), e.g. the hangar's own cube map. Materials that
    /// name `/material/environment/vehicle_reflection.tobj` sample that slot
    /// (Hypothesis, research/evidence/exposure-tonemap-data.txt section 4).
    pub vehicle_reflection: Option<String>,
    /// `cube_reflection`: Hypothesis, the engine renders a reflection cube of
    /// the scene itself at runtime (F16).
    pub cube_reflection: bool,
    /// `floor_reflection`: the floor mirrors the truck; the game forces it off
    /// when the scene has no parts to customise for reflection (F16).
    pub floor_reflection: bool,
    pub fog_color: Option<[f32; 3]>,
    pub fog_density: Option<f32>,
    pub truck_position: [f32; 3],
    /// Euler angles in degrees.
    pub truck_rotation: [f32; 3],
}

/// Every scene definition, the default garage first. Files that fail to
/// parse are skipped.
pub fn list_scenes(fs: &GameFs) -> Result<Vec<Scene>> {
    let mut files: Vec<String> = fs
        .list_dir(SCENE_DIR)?
        .files
        .into_iter()
        .filter(|f| f.starts_with("config") && f.ends_with(".sii"))
        .collect();
    files.sort_by_key(|f| (f != DEFAULT_SCENE_FILE, f.clone()));
    let mut scenes = Vec::new();
    for file in files {
        let path = format!("{SCENE_DIR}/{file}");
        let Ok(doc) = fs.read_sii(&path) else {
            continue;
        };
        let unit = doc.units_of_class(SCENE_CLASS).next();
        if let Some(unit) = unit {
            scenes.push(scene_from_unit(fs, &path, unit));
        }
    }
    Ok(scenes)
}

fn scene_from_unit(fs: &GameFs, path: &str, unit: &Unit) -> Scene {
    let vec3 = |key: &str| unit.get(key).and_then(floats3);
    let model = unit.get_str("scene_model_path").map(|model| SceneModel {
        path: model.to_owned(),
        look: unit.get_str("scene_model_look").map(str::to_owned),
        variant: unit.get_str("scene_model_variant").map(str::to_owned),
        position: vec3("scene_model_position").unwrap_or([0.0; 3]),
        parts: unit
            .array_str("part_order")
            .into_iter()
            .map(str::to_owned)
            .collect(),
    });
    // Scenes carry a static pose and cinematic animations; any of them can
    // be a placeholder (the garage's static pose sits inside the truck) or a
    // fly-through aimed away from it. Use the first pose (static, then each
    // animation's first frame) that frames the truck.
    let static_pose = vec3("camera_position")
        .zip(vec3("camera_aim"))
        .map(|(position, aim)| CameraPose {
            position,
            aim,
            fov: unit.get_f32("camera_fov").unwrap_or(45.0),
        });
    let cinematic = unit
        .get_str("scene_cinematic_camera")
        .and_then(|cinematic| fs.read_sii(cinematic).ok())
        .map(|doc| cinematic_poses(&doc))
        .unwrap_or_default();
    let truck = vec3("truck_position").unwrap_or([0.0; 3]);
    let camera = static_pose
        .into_iter()
        .chain(cinematic)
        .find_map(|pose| framing(&pose, truck));
    Scene {
        path: path.to_owned(),
        name: unit.get_str("scene_name").unwrap_or(path).to_owned(),
        model,
        camera,
        light_direction: vec3("light_direction"),
        light_ambient: vec3("light_ambient"),
        light_diffuse: vec3("light_diffuse"),
        light_specular: vec3("light_specular"),
        light_environment: unit.get_f32("light_environment"),
        pre_exposure: unit.get_f32("pre_exposure").filter(|v| v.is_finite()),
        vehicle_reflection: unit
            .get_str("vehicle_reflection")
            .filter(|path| !path.is_empty())
            .map(str::to_owned),
        cube_reflection: unit.get_bool("cube_reflection").unwrap_or(false),
        floor_reflection: unit.get_bool("floor_reflection").unwrap_or(false),
        fog_color: vec3("fog_color"),
        fog_density: unit.get_f32("fog_density"),
        truck_position: vec3("truck_position").unwrap_or([0.0; 3]),
        truck_rotation: vec3("truck_rot").unwrap_or([0.0; 3]),
    }
}

/// Frame 0 of every camera animation, in order, each with its camera's
/// field of view.
fn cinematic_poses(doc: &SiiDocument) -> Vec<CameraPose> {
    let Some(cinematic) = doc.units_of_class("cinematic_camera").next() else {
        return Vec::new();
    };
    let cameras = cinematic.array_str("cameras");
    cinematic
        .array_str("animations")
        .into_iter()
        .filter_map(|name| doc.unit(name))
        .filter_map(|animation| {
            let position = animation
                .array("positions")
                .first()
                .and_then(|v| floats3(v))?;
            let aim = animation.array("aims").first().and_then(|v| floats3(v))?;
            let fov = usize::try_from(animation.get_i64("camera_index").unwrap_or(0))
                .ok()
                .and_then(|index| cameras.get(index))
                .and_then(|name| doc.unit(name))
                .and_then(|camera| camera.get_f32("camera_fov"))
                .unwrap_or(45.0);
            Some(CameraPose { position, aim, fov })
        })
        .collect()
}

/// Radius (metres) of the sphere around the truck that a usable camera must
/// stand outside of and look through.
const TRUCK_RADIUS: f32 = 5.0;
/// Height of the truck's visual centre above its origin.
const TRUCK_CENTER_HEIGHT: f32 = 1.5;

/// `pose` re-aimed at the point of its view ray closest to the truck, if the
/// camera stands outside the truck and its ray passes through it. Scene aims
/// are often far points along the ray, unusable as an orbit target.
fn framing(pose: &CameraPose, truck: [f32; 3]) -> Option<CameraPose> {
    let center = [truck[0], truck[1] + TRUCK_CENTER_HEIGHT, truck[2]];
    let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let direction = sub(pose.aim, pose.position);
    let length_sq = dot(direction, direction);
    let to_center = sub(center, pose.position);
    if length_sq <= f32::EPSILON || dot(to_center, to_center).sqrt() <= TRUCK_RADIUS {
        return None;
    }
    let t = dot(to_center, direction) / length_sq;
    if t <= 0.0 {
        return None; // looking away from the truck
    }
    let closest = [0, 1, 2].map(|i| pose.position[i] + direction[i] * t);
    let miss = sub(center, closest);
    (dot(miss, miss).sqrt() <= TRUCK_RADIUS).then_some(CameraPose {
        position: pose.position,
        aim: closest,
        fov: pose.fov,
    })
}

/// The first three components, if all are finite.
fn floats3(value: &Value) -> Option<[f32; 3]> {
    let v: [f32; 3] = value.as_floats()?.get(..3)?.try_into().ok()?;
    v.iter().all(|c| c.is_finite()).then_some(v)
}

/// The first two components, if both are finite.
fn floats2(value: &Value) -> Option<[f32; 2]> {
    let v: [f32; 2] = value.as_floats()?.get(..2)?.try_into().ok()?;
    v.iter().all(|c| c.is_finite()).then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use specsheet_formats::sii;

    #[test]
    fn camera_must_stand_outside_and_look_through_the_truck() {
        let pose = |position, aim| CameraPose {
            position,
            aim,
            fov: 45.0,
        };
        let truck = [0.0; 3];
        // Garage placeholder: inside the truck.
        assert_eq!(
            framing(&pose([0.0, 3.0, 0.0], [0.0, 3.0, 1.0]), truck),
            None
        );
        // Looking away from the truck.
        assert_eq!(
            framing(&pose([0.0, 1.5, -10.0], [0.0, 1.5, -20.0]), truck),
            None
        );
        // Under the bridge, static pose: in front, aiming at the cab.
        let framed = framing(&pose([0.0, 3.1, -15.0], [0.0, 2.5, -0.7]), truck).expect("frames");
        assert!(framed.aim[2].abs() < 1.0, "{:?}", framed.aim);
        // Night City: a far aim point whose ray crosses the truck is re-aimed.
        let far =
            framing(&pose([3.65, 1.88, -8.87], [-277.3, -4.8, 576.1]), truck).expect("frames");
        assert!(far.aim.iter().all(|c| c.abs() < 6.0), "{:?}", far.aim);
    }

    #[test]
    fn reads_the_tuning_screen_orbit_and_parallel_pin_arrays() {
        let src = br#"SiiNunit
{
ui_truck_scene_config : _nameless.config {
camera_aim: ( 0.0, 2.6, -1.465 )
camera_elevation_limit: (-0.35, 0.0)
camera_relative_bounds: (7.5, 7.5, 7.5)
camera_fov: 50
camera_default_azimuth: -2.516
camera_default_elevation: -0.068
camera_default_zoom: 1
scene_model_path: "/model/ui/garage/g.pmd"
pin_group[]: air_tank
pin_look[]: custom_acc
pin_offset[]: (0.0, 0.0, 0.0)
pin_dir[]: (0, 0, 1)
pin_dir_uk[]: (0, 0, 1)
double_sided[]: false
pin_group[]: f_tire
pin_look[]: custom_acc
pin_offset[]: (-0.1, 0.4, 0.0)
pin_dir[]: (-1, 0, 0)
pin_dir_uk[]: (-1, 0, 0)
double_sided[]: true
interior_only[]: carpet
hookups_exterior_only[]: sunshld
}
}"#;
        let doc = sii::parse("/def/s.sii", src).expect("parse");
        let unit = doc.units_of_class(TUNING_SCENE_CLASS).next().expect("unit");
        let fs = GameFs::new(specsheet_archive::Vfs::new());
        let tuning = tuning_scene_from_unit(&fs, "/def/s.sii", unit);
        let orbit = tuning.orbit.expect("orbit");
        assert_eq!(orbit.fov, 50.0);
        assert_eq!(orbit.elevation_limit, [-0.35, 0.0]);
        // Front-left three-quarter view, slightly above the aim, ~7.5 m out.
        let [x, y, z] = orbit.position();
        assert!(x < -3.0 && z < -5.0 && y > orbit.aim[1], "{:?}", [x, y, z]);
        let camera = tuning.scene.camera.expect("camera");
        assert_eq!(camera.aim, orbit.aim);
        assert_eq!(tuning.pins.len(), 2);
        assert_eq!(tuning.pins[1].group, "f_tire");
        assert_eq!(tuning.pins[1].offset, [-0.1, 0.4, 0.0]);
        assert!(tuning.pins[1].double_sided && !tuning.pins[0].double_sided);
        assert_eq!(tuning.interior_only, vec!["carpet".to_owned()]);
        assert_eq!(tuning.hookups_exterior_only, vec!["sunshld".to_owned()]);
    }

    #[test]
    fn reads_the_scene_reflection_keys() {
        // `/def/desktop/config.hangar.sii` and `/def/truck_config_scene.sii`
        // (research/evidence/exposure-tonemap-data.txt).
        let src = br#"SiiNunit
{
ui_truck_scene_base_config : hangar.config {
vehicle_reflection: "/model/ui/hangar/hangar_cubemap.tobj"
pre_exposure: 1.0
}
ui_truck_scene_base_config : plain.config {
cube_reflection: true
floor_reflection: true
}
}"#;
        let doc = sii::parse("/def/s.sii", src).expect("parse");
        let fs = GameFs::new(specsheet_archive::Vfs::new());
        let mut units = doc.units_of_class("ui_truck_scene_base_config");
        let hangar = scene_from_unit(&fs, "/def/s.sii", units.next().expect("hangar"));
        assert_eq!(
            hangar.vehicle_reflection.as_deref(),
            Some("/model/ui/hangar/hangar_cubemap.tobj")
        );
        assert_eq!(hangar.pre_exposure, Some(1.0));
        assert!(!hangar.cube_reflection && !hangar.floor_reflection);
        let plain = scene_from_unit(&fs, "/def/s.sii", units.next().expect("plain"));
        assert_eq!((plain.vehicle_reflection, plain.pre_exposure), (None, None));
        assert!(plain.cube_reflection && plain.floor_reflection);
    }

    #[test]
    fn non_finite_orbit_values_fall_back_to_defaults() {
        // `&7fc00000` is a NaN and `&7f800000` +inf as SII hex floats.
        let src = br#"SiiNunit
{
ui_truck_scene_config : _nameless.config {
camera_aim: ( 0.0, &7fc00000, 0.0 )
camera_elevation_limit: (&7fc00000, &7fc00000)
camera_relative_bounds: (&7f800000, 7.5, 7.5)
camera_default_azimuth: &7fc00000
camera_default_zoom: &7f800000
}
}"#;
        let doc = sii::parse("/def/s.sii", src).expect("parse");
        let unit = doc.units_of_class(TUNING_SCENE_CLASS).next().expect("unit");
        let fs = GameFs::new(specsheet_archive::Vfs::new());
        let tuning = tuning_scene_from_unit(&fs, "/def/s.sii", unit);
        // A non-finite aim leaves no usable camera.
        assert_eq!(tuning.orbit, None);
        let src = String::from_utf8_lossy(src).replace("( 0.0, &7fc00000, 0.0 )", "( 0, 2, 0 )");
        let doc = sii::parse("/def/s.sii", src.as_bytes()).expect("parse");
        let unit = doc.units_of_class(TUNING_SCENE_CLASS).next().expect("unit");
        let orbit = tuning_scene_from_unit(&fs, "/def/s.sii", unit)
            .orbit
            .expect("orbit");
        assert!(orbit.elevation_limit.iter().all(|v| v.is_finite()));
        assert!(orbit.relative_bounds.iter().all(|v| v.is_finite()));
        assert!(orbit.azimuth.is_finite() && orbit.zoom.is_finite());
        assert!(orbit.position().iter().all(|v| v.is_finite()));
    }

    #[test]
    fn takes_frame_zero_of_every_camera_animation() {
        let src = br#"SiiNunit
{
cinematic_camera: _nameless.cinematic.camera {
cameras: 2
cameras[0]: _nameless.camera0
cameras[1]: _nameless.camera1
animations: 1
animations[0]: _nameless.anim0
}
orbit_camera: _nameless.camera0 { camera_fov: 40 }
orbit_camera: _nameless.camera1 { camera_fov: 55 }
camera_animation_info: _nameless.anim0 {
camera_index: 1
positions: 2
positions[0]: ( -7.5, 1.8, -9.5 )
positions[1]: ( -7.6, 1.8, -9.4 )
aims: 2
aims[0]: ( -2.0, 1.5, -4.0 )
aims[1]: ( -2.1, 1.5, -4.0 )
}
}"#;
        let doc = sii::parse("/def/camera/ui/x.sii", src).expect("parse");
        assert_eq!(
            cinematic_poses(&doc),
            vec![CameraPose {
                position: [-7.5, 1.8, -9.5],
                aim: [-2.0, 1.5, -4.0],
                fov: 55.0,
            }]
        );
    }
}
