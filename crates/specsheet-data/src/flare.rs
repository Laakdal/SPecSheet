//! The vehicle light hookups: the `flare.vehicle.*` units a model locator's
//! hookup names (e.g. `flare.vehicle.headl` on a headlight locator), from
//! `/unit/hookup/vehicle/flare/*.sii`. Each names the light it belongs to
//! (`light_type`, e.g. `low_beam`) and usually a glow model under
//! `/model/flare/` (effect `eut2.flare`). How the engine draws and switches
//! them is not specified yet (`docs/app/shader-specs/`).

use std::collections::BTreeMap;

use specsheet_formats::sii::Unit;

use crate::error::Result;
use crate::fs::GameFs;

/// Where the vehicle light units live.
pub const VEHICLE_LIGHT_DIR: &str = "/unit/hookup/vehicle/flare";

/// The unit class of a vehicle light.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightKind {
    /// `flare_vehicle`: a steady light (head, rear, brake, park lights…).
    Flare,
    /// `flare_blink`: a blinking light (blinkers, hazard lights).
    Blink,
    /// `beacon_vehicle`: a rotating beacon.
    Beacon,
}

impl LightKind {
    fn of(class: &str) -> Option<Self> {
        match class {
            "flare_vehicle" => Some(Self::Flare),
            "flare_blink" => Some(Self::Blink),
            "beacon_vehicle" => Some(Self::Beacon),
            _ => None,
        }
    }
}

/// One vehicle light unit.
#[derive(Clone, Debug, PartialEq)]
pub struct VehicleLight {
    /// The unit name a locator's hookup names, e.g. `flare.vehicle.headl`.
    pub name: String,
    /// Its class.
    pub kind: LightKind,
    /// The light it belongs to (`light_type`, e.g. `low_beam`, `brake`).
    pub light_type: Option<String>,
    /// The glow model (`model`), e.g. `/model/flare/uniform_white_amplified.pmd`;
    /// `None` for units that only light the scene.
    pub model: Option<String>,
    /// `dir_type`: `narrow` (a cone around the locator's forward axis) or
    /// `wide` (flare.md §4, Hypothesis).
    pub dir_type: Option<String>,
    /// `flare_inner_angle` in degrees: full strength inside it (Hypothesis).
    pub inner_angle: Option<f32>,
    /// `default_scale`: the glow quad's size in metres near the camera
    /// (Hypothesis).
    pub default_scale: Option<f32>,
    /// `scale_factor`: the size grows to `default_scale × scale_factor`
    /// between `scaling_start_distance` and `scaling_end_distance`
    /// (Hypothesis).
    pub scale_factor: Option<f32>,
    /// `scaling_start_distance`, `scaling_end_distance` in metres.
    pub scaling_distances: Option<[f32; 2]>,
    /// `visual_offset`: metres along the locator's forward axis, out of the
    /// lamp housing (Hypothesis).
    pub visual_offset: Option<f32>,
}

fn light_from_unit(unit: &Unit) -> Option<VehicleLight> {
    let number = |key: &str| unit.get_f32(key).filter(|v| v.is_finite());
    Some(VehicleLight {
        name: unit.name.clone(),
        kind: LightKind::of(&unit.class)?,
        light_type: unit.get_str("light_type").map(str::to_owned),
        model: unit
            .get_str("model")
            .filter(|path| !path.is_empty())
            .map(str::to_owned),
        dir_type: unit.get_str("dir_type").map(str::to_owned),
        inner_angle: number("flare_inner_angle"),
        default_scale: number("default_scale"),
        scale_factor: number("scale_factor"),
        scaling_distances: number("scaling_start_distance")
            .zip(number("scaling_end_distance"))
            .map(|(start, end)| [start, end]),
        visual_offset: number("visual_offset"),
    })
}

/// Every vehicle light unit by name. Files that fail to read are reported
/// in the warnings and skipped.
pub fn vehicle_lights(fs: &GameFs) -> Result<(BTreeMap<String, VehicleLight>, Vec<String>)> {
    let mut lights = BTreeMap::new();
    let mut warnings = Vec::new();
    for file in fs.list_dir(VEHICLE_LIGHT_DIR)?.files {
        if !file.ends_with(".sii") {
            continue;
        }
        let path = format!("{VEHICLE_LIGHT_DIR}/{file}");
        match fs.read_sii(&path) {
            Ok(doc) => {
                for light in doc.units.iter().filter_map(light_from_unit) {
                    lights.insert(light.name.clone(), light);
                }
            }
            Err(error) => warnings.push(format!("{path}: {error}")),
        }
    }
    Ok((lights, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use specsheet_formats::sii;

    #[test]
    fn reads_the_three_light_classes_and_skips_others() {
        // Shaped like `vehicle_headl.sii`, a blinker and a beacon.
        let src = br#"SiiNunit
{
flare_vehicle: flare.vehicle.headl
{
	dir_type: narrow
	flare_inner_angle: 7
	light_type: low_beam
	model: "/model/flare/uniform_white_amplified.pmd"
	visual_offset: 0.6
	default_scale: 1.53
	scale_factor: 3
	scaling_start_distance: 0
	scaling_end_distance: 60.0
}
flare_blink: flare.vehicle.lblinkerl
{
	light_type: left_blinker
	model: "/model/flare/orange.pmd"
}
beacon_vehicle: flare.vehicle.beacon
{
	light_type: beacon
	model: ""
}
particle_gen: some.particles
{
}
}"#;
        let doc = sii::parse("/unit/hookup/vehicle/flare/x.sii", src).expect("parse");
        let lights: Vec<VehicleLight> = doc.units.iter().filter_map(light_from_unit).collect();
        assert_eq!(lights.len(), 3);
        assert_eq!(lights[0].name, "flare.vehicle.headl");
        assert_eq!(lights[0].kind, LightKind::Flare);
        assert_eq!(lights[0].light_type.as_deref(), Some("low_beam"));
        assert_eq!(
            lights[0].model.as_deref(),
            Some("/model/flare/uniform_white_amplified.pmd")
        );
        assert_eq!(lights[0].dir_type.as_deref(), Some("narrow"));
        assert_eq!(lights[0].inner_angle, Some(7.0));
        assert_eq!(lights[0].default_scale, Some(1.53));
        assert_eq!(lights[0].scale_factor, Some(3.0));
        assert_eq!(lights[0].scaling_distances, Some([0.0, 60.0]));
        assert_eq!(lights[0].visual_offset, Some(0.6));
        assert_eq!(lights[1].kind, LightKind::Blink);
        assert_eq!(lights[1].default_scale, None);
        assert_eq!(
            (lights[2].kind, lights[2].model.as_deref()),
            (LightKind::Beacon, None)
        );
    }
}
