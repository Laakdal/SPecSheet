//! The trucks' interior cameras: the `vehicle_interior_camera` unit a
//! truck's `data.sii` names in `interior_camera` (e.g.
//! `camera.interior.renault.t`). The units are written in
//! `/def/camera/units/interior_*.sui` and included by the
//! `/def/camera/camera_storage*.sii` files. Each carries the driver's view
//! for its cockpit: the field of view, a head offset from the cabin's eye
//! locator and the starting look. The tuning screen's interior tabs use the
//! truck's own unit, without the profile's seat adjustment
//! (`research/evidence/rea-interior-camera.txt`).

use std::collections::BTreeMap;

use specsheet_core::TruckId;
use specsheet_formats::sii::Unit;

use crate::error::Result;
use crate::fs::GameFs;

/// Where the camera storage files live.
pub const CAMERA_DIR: &str = "/def/camera";
/// The file name prefix of the camera storage files.
const STORAGE_PREFIX: &str = "camera_storage";
/// The unit class of a truck's interior camera.
pub const INTERIOR_CAMERA_CLASS: &str = "vehicle_interior_camera";
/// Where the truck definitions live.
const TRUCK_DEFS: &str = "/def/vehicle/truck";

/// One interior camera unit.
#[derive(Clone, Debug, PartialEq)]
pub struct InteriorCamera {
    /// The unit name, e.g. `camera.interior.renault.t`.
    pub name: String,
    /// `camera_fov`, in degrees.
    pub fov: Option<f32>,
    /// `head_offset`: the head's offset from the eye locator, in metres
    /// (x right, y up, z back); zero when absent.
    pub head_offset: [f32; 3],
    /// `mouse_up_down_default`: the starting pitch, in degrees.
    pub default_pitch: Option<f32>,
    /// `mouse_left_right_default`: the starting yaw, in degrees.
    pub default_yaw: Option<f32>,
}

impl InteriorCamera {
    /// Reads a `vehicle_interior_camera` unit.
    pub fn from_unit(unit: &Unit) -> Self {
        let head_offset = unit
            .get("head_offset")
            .and_then(|v| v.as_floats())
            .and_then(|v| <[f32; 3]>::try_from(v).ok())
            .unwrap_or([0.0; 3]);
        Self {
            name: unit.name.clone(),
            fov: unit.get_f32("camera_fov"),
            head_offset,
            default_pitch: unit.get_f32("mouse_up_down_default"),
            default_yaw: unit.get_f32("mouse_left_right_default"),
        }
    }

    /// The head's offset from the eye locator, in truck axes: the game adds
    /// `head_offset` unrotated and mirrors its x in a right-hand-drive cab.
    pub fn head_offset(&self, right_hand_drive: bool) -> [f32; 3] {
        let [x, y, z] = self.head_offset;
        [if right_hand_drive { -x } else { x }, y, z]
    }

    /// The starting look as (yaw, pitch) in degrees: the two mouse
    /// defaults, the yaw mirrored in a right-hand-drive cab. Absent values
    /// are 0.
    pub fn start_look(&self, right_hand_drive: bool) -> (f32, f32) {
        let yaw = self.default_yaw.unwrap_or(0.0);
        let yaw = if right_hand_drive { -yaw } else { yaw };
        (yaw, self.default_pitch.unwrap_or(0.0))
    }

    /// The vertical field of view in degrees for a view `aspect` wide
    /// (width / height). `camera_fov` is the horizontal angle of a 4:3
    /// view: wider views keep that view's vertical angle, narrower ones
    /// keep the horizontal angle.
    pub fn vertical_fov(&self, aspect: f32) -> Option<f32> {
        let half = (self.fov? / 2.0).to_radians().tan();
        let tan_vertical = if aspect >= 4.0 / 3.0 {
            0.75 * half
        } else {
            half / aspect
        };
        Some(2.0 * tan_vertical.atan().to_degrees())
    }
}

/// Every interior camera unit of the camera storage files, by name. A
/// storage file that fails to parse is skipped (a mod's broken file must
/// not hide the base game's cameras).
pub fn interior_cameras(fs: &GameFs) -> Result<BTreeMap<String, InteriorCamera>> {
    let mut cameras = BTreeMap::new();
    let mut files: Vec<String> = fs
        .list_dir(CAMERA_DIR)?
        .files
        .into_iter()
        .filter(|f| f.starts_with(STORAGE_PREFIX) && f.ends_with(".sii"))
        .collect();
    files.sort();
    for file in files {
        let Ok(doc) = fs.read_sii(&format!("{CAMERA_DIR}/{file}")) else {
            continue;
        };
        for unit in doc.units_of_class(INTERIOR_CAMERA_CLASS) {
            cameras.insert(unit.name.clone(), InteriorCamera::from_unit(unit));
        }
    }
    Ok(cameras)
}

/// The interior camera unit `truck`'s `data.sii` names, if any.
pub fn interior_camera_name(fs: &GameFs, truck: &TruckId) -> Result<Option<String>> {
    let doc = fs.read_sii(&format!("{TRUCK_DEFS}/{truck}/data.sii"))?;
    let name = doc
        .units_of_class("accessory_truck_data")
        .next()
        .and_then(|u| u.get_str("interior_camera"))
        .map(str::to_owned);
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use specsheet_formats::sii;

    #[test]
    fn reads_the_view_of_an_interior_camera_unit() {
        // Shaped like `interior_renault_t.sui`.
        let src = br#"SiiNunit
{
vehicle_interior_camera: camera.interior.t
{
	mouse_right_limit: -180
	camera_fov: 72
	head_offset: (0, -0.17, 0.01)
	mouse_up_down_default: -5.5
	look_presets_defaults[]: 0
}
vehicle_interior_camera: camera.interior.bare
{
	near_plane: 0.1
}
}
"#;
        let doc = sii::parse("units.sui", src).expect("parse");
        let cameras: Vec<_> = doc
            .units_of_class(INTERIOR_CAMERA_CLASS)
            .map(InteriorCamera::from_unit)
            .collect();
        assert_eq!(
            cameras[0],
            InteriorCamera {
                name: "camera.interior.t".to_owned(),
                fov: Some(72.0),
                head_offset: [0.0, -0.17, 0.01],
                default_pitch: Some(-5.5),
                default_yaw: None,
            }
        );
        assert_eq!(cameras[1].head_offset, [0.0; 3]);
        assert_eq!((cameras[1].fov, cameras[1].default_pitch), (None, None));
    }

    fn camera() -> InteriorCamera {
        InteriorCamera {
            name: "camera.interior.t".to_owned(),
            fov: Some(72.0),
            head_offset: [0.02, -0.17, 0.01],
            default_pitch: Some(-5.5),
            default_yaw: Some(10.0),
        }
    }

    #[test]
    fn a_right_hand_drive_cab_mirrors_the_head_offset_and_the_yaw() {
        let camera = camera();
        assert_eq!(camera.head_offset(false), [0.02, -0.17, 0.01]);
        assert_eq!(camera.head_offset(true), [-0.02, -0.17, 0.01]);
        assert_eq!(camera.start_look(false), (10.0, -5.5));
        assert_eq!(camera.start_look(true), (-10.0, -5.5));
    }

    #[test]
    fn the_fov_is_the_horizontal_angle_of_a_4_3_view() {
        // rea-interior-camera.txt: fov 72 on 16:9 gives 57.2° vertical.
        let camera = camera();
        let wide = camera.vertical_fov(16.0 / 9.0).expect("fov");
        assert!((wide - 57.2).abs() < 0.05, "{wide}");
        assert_eq!(camera.vertical_fov(4.0 / 3.0), camera.vertical_fov(2.0));
        // Narrower than 4:3 the horizontal angle stays 72°.
        let square = camera.vertical_fov(1.0).expect("fov");
        assert!((square - 72.0).abs() < 1e-3, "{square}");
    }
}
