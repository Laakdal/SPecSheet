//! The game's truck-tuning ("desktop") scenes: `/def/desktop/config*.sii`
//! (`ui_truck_scene_base_config`, finding F7) and their cinematic cameras
//! (`/def/camera/ui/*.sii`, `cinematic_camera` / `orbit_camera` /
//! `camera_animation_info`).

use specsheet_formats::sii::{SiiDocument, Unit, Value};

use crate::error::Result;
use crate::fs::GameFs;

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

fn floats3(value: &Value) -> Option<[f32; 3]> {
    value.as_floats()?.get(..3)?.try_into().ok()
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
