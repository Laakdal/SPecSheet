//! Assembles a configured truck into model placements and hotspot anchors.
//!
//! * Chassis and cabin models share one coordinate space and are placed at
//!   the origin (finding F4).
//! * Addons with an exterior model are placed at every visible base-model
//!   locator of their slot ([`crate::slots`]).
//! * Wheel parts are placed at the chassis `wheel_f*` / `wheel_r*` locators;
//!   their models have `left`/`right` variants, chosen by the locator side.
//!   Tyres are offset by the rim's `tire_shift` (finding F9).
//! * Hookups are placed at the `slot_NN` locators of their host accessory's
//!   model, composed with the host's own placement (finding F11).
//! * The selected interior's light exterior-view model (`ext_model`) is
//!   placed at the cabin's `ext_interior` locator, so the cab is not hollow
//!   when seen through the windows (`research/evidence/interior-survey.txt`).

mod anchors;
mod base;
mod hookups;
mod interior;
mod transform;
mod wheels;

use std::collections::BTreeMap;

use specsheet_core::{
    AccessoryId, AccessoryKind, Configuration, SlotAvailability, SlotId, TruckCatalog,
};
use specsheet_formats::pmg::Locator;

pub use anchors::Anchor;
pub use base::slot_availability;
pub use hookups::HookupPosition;
pub use transform::Transform;

use crate::fs::GameFs;
use crate::model::ModelCache;
use crate::slots::locators_by_slot;
use anchors::{anchors, placed_boxes};
use base::{addon_slots, base_layout};
use hookups::hookup_placements;
use interior::interior_placements;
use wheels::wheel_placements;

/// One model instance of the assembled truck.
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub slot: SlotId,
    pub accessory: AccessoryId,
    pub model: String,
    pub look: Option<String>,
    pub variant: Option<String>,
    pub transform: Transform,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Assembly {
    pub placements: Vec<Placement>,
    /// Hookup positions offered by each placed host accessory, by slot.
    pub hookup_positions: BTreeMap<SlotId, Vec<HookupPosition>>,
    pub availability: SlotAvailability,
    pub anchors: Vec<Anchor>,
    /// Non-fatal problems (missing models, unmatched slots).
    pub warnings: Vec<String>,
}

/// Right-hand-drive (UK) or left-hand-drive assembly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Drive {
    #[default]
    Left,
    Right,
}

const FRONT_WHEELS: &str = "wheel_f";
const REAR_WHEELS: &str = "wheel_r";

/// Assembles `config`. Base models that fail to load are reported as
/// warnings; the rest of the truck is still assembled.
pub fn assemble(
    fs: &GameFs,
    cache: &ModelCache,
    catalog: &TruckCatalog,
    config: &Configuration,
    drive: Drive,
) -> Assembly {
    let mut assembly = Assembly::default();
    let selected: Vec<_> = config
        .selections()
        .values()
        .filter_map(|id| catalog.accessory(id))
        .collect();

    let base = base_layout(fs, cache, catalog, config, drive);
    assembly.warnings = base.warnings;
    assembly.placements = base.placements;
    let base_locators = base.locators;
    let base_boxes = base.boxes;

    let addon_slots = addon_slots(catalog);
    let by_slot = locators_by_slot(&addon_slots, base_locators.iter());
    assembly.availability = SlotAvailability {
        available: by_slot.keys().cloned().collect(),
    };

    for accessory in selected.iter().filter(|a| a.needs_exterior_locator()) {
        let model = match drive {
            Drive::Right => accessory
                .model
                .exterior_uk
                .as_ref()
                .or(accessory.model.exterior.as_ref()),
            Drive::Left => accessory.model.exterior.as_ref(),
        };
        let (Some(model), Some(locators)) = (model, by_slot.get(&accessory.slot)) else {
            assembly.warnings.push(format!(
                "{}: no locator for slot {}",
                accessory.id, accessory.slot
            ));
            continue;
        };
        for locator in locators {
            assembly.placements.push(Placement {
                slot: accessory.slot.clone(),
                accessory: accessory.id.clone(),
                model: model.clone(),
                look: accessory.model.look.clone(),
                variant: accessory.model.variant.clone(),
                transform: Transform::from_locator(locator),
            });
        }
    }

    // Wheels.
    let wheel_locators = |prefix: &str| -> Vec<&Locator> {
        base_locators
            .iter()
            .filter(|l| l.name.starts_with(prefix))
            .collect()
    };
    let front = wheel_locators(FRONT_WHEELS);
    let rear = wheel_locators(REAR_WHEELS);
    let wheels = wheel_placements(&selected, &front, &rear, &mut assembly.warnings);
    assembly.placements.extend(wheels);
    let interior = interior_placements(&selected, &base_locators, drive, &mut assembly.warnings);
    assembly.placements.extend(interior);
    let (positions, hookups) = hookup_placements(fs, cache, catalog, config, &assembly.placements);
    assembly.hookup_positions = positions;
    for (placement, warning) in hookups {
        match placement {
            Some(placement) => assembly.placements.push(placement),
            None => assembly.warnings.extend(warning),
        }
    }

    let cabin_box = base_boxes
        .iter()
        .find(|(_, a)| a.kind == AccessoryKind::Cabin)
        .map(|(aabb, _)| *aabb);
    let slot_boxes = placed_boxes(fs, cache, &assembly.placements);
    assembly.anchors = anchors(
        catalog,
        &by_slot,
        &slot_boxes,
        &base_boxes,
        cabin_box,
        &front,
        &rear,
    );
    assembly
}

#[cfg(test)]
mod tests {
    use specsheet_core::{AccessoryId, AccessoryKind, SlotId, WheelPart};
    use specsheet_formats::pmg::{Aabb, Locator};

    use super::transform::{rotate, transform_aabb};
    use super::*;

    fn wheel_part(id: &str, slot: &str, wheel: WheelPart) -> specsheet_core::Accessory {
        specsheet_core::Accessory {
            id: AccessoryId::from(id),
            slot: SlotId::from(slot),
            kind: AccessoryKind::Wheel,
            def_path: format!("/def/vehicle/{slot}/{id}.sii"),
            name: id.to_owned(),
            icon: None,
            price: None,
            unlock_level: None,
            model: specsheet_core::ModelRef {
                exterior: Some(format!("/vehicle/{id}.pmd")),
                ..specsheet_core::ModelRef::default()
            },
            suitable_for: Vec::new(),
            conflict_with: Vec::new(),
            defaults: Vec::new(),
            requires: Vec::new(),
            base_color: None,
            wheel: Some(wheel),
        }
    }

    fn locator(name: &str, position: [f32; 3]) -> Locator {
        Locator {
            name: name.to_owned(),
            position,
            scale: 1.0,
            rotation_wxyz: [1.0, 0.0, 0.0, 0.0],
            hookup: None,
        }
    }

    /// Finding F9: the tyre moves by the disc's `tire_shift` towards the
    /// truck's centre line (+x on the left, -x on the right); the disc and
    /// the other hub-face parts stay at the locator.
    #[test]
    fn tyres_move_by_the_rim_tire_shift() {
        let disc = wheel_part(
            "5.fdisc",
            "f_disc",
            WheelPart::Rim {
                tire_shift: Some(-0.038),
            },
        );
        let hub = wheel_part("1.fhub", "f_hub", WheelPart::Rim { tire_shift: None });
        let tyre = wheel_part("1.ftire", "f_tire", WheelPart::Tire);
        let rear_disc = wheel_part(
            "1.rdisc",
            "r_disc",
            WheelPart::Rim {
                tire_shift: Some(-0.01),
            },
        );
        let rear_tyre = wheel_part("1.rtire", "r_tire", WheelPart::Tire);
        let left = locator("wheel_f_0", [-1.0, 0.5, -1.6]);
        let right = locator("wheel_f_1", [1.0, 0.5, -1.6]);
        let rear_left = locator("wheel_r_0", [-0.9, 0.5, 2.0]);
        let mut warnings = Vec::new();
        let placements = wheel_placements(
            &[&disc, &hub, &tyre, &rear_disc, &rear_tyre],
            &[&left, &right],
            &[&rear_left],
            &mut warnings,
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        let x = |id: &str, variant: &str| -> f32 {
            placements
                .iter()
                .find(|p| p.accessory.as_str() == id && p.variant.as_deref() == Some(variant))
                .map(|p| p.transform.translation[0])
                .unwrap_or(f32::NAN)
        };
        let close = |a: f32, b: f32| (a - b).abs() < 1e-6;
        assert!(
            close(x("1.ftire", "left"), -1.038),
            "{}",
            x("1.ftire", "left")
        );
        assert!(
            close(x("1.ftire", "right"), 1.038),
            "{}",
            x("1.ftire", "right")
        );
        assert!(close(x("5.fdisc", "left"), -1.0));
        assert!(close(x("1.fhub", "right"), 1.0));
        assert!(
            close(x("1.rtire", "left"), -0.91),
            "{}",
            x("1.rtire", "left")
        );
        assert!(close(x("1.rdisc", "left"), -0.9));
    }

    /// 11 of 562 chassis wheel locators carry a 90 or 180 degree yaw
    /// (MAN TGX, Renault Magnum, Iveco Stralis); wheels keep the truck's
    /// orientation and only take the locator's position.
    #[test]
    fn wheels_ignore_locator_rotation() {
        let tyre = wheel_part("1.rtire", "r_tire", WheelPart::Tire);
        let disc = wheel_part(
            "1.rdisc",
            "r_disc",
            WheelPart::Rim {
                tire_shift: Some(-0.01),
            },
        );
        let mut yawed = locator("wheel_r_1", [0.93, 0.5, 2.0]);
        yawed.rotation_wxyz = [0.0, 0.0, 1.0, 0.0];
        let mut warnings = Vec::new();
        let placements = wheel_placements(&[&tyre, &disc], &[], &[&yawed], &mut warnings);
        for placement in &placements {
            assert_eq!(placement.transform.rotation, [0.0, 0.0, 0.0, 1.0]);
            assert_eq!(placement.variant.as_deref(), Some("right"));
        }
        let tyre_x = placements
            .iter()
            .find(|p| p.accessory.as_str() == "1.rtire")
            .map(|p| p.transform.translation[0]);
        assert!(
            tyre_x.is_some_and(|x| (x - 0.94).abs() < 1e-6),
            "{tyre_x:?}"
        );
    }

    #[test]
    fn interior_exterior_model_sits_at_the_ext_interior_locator() {
        let mut interior = wheel_part("std.t.interior", "interior", WheelPart::Tire);
        interior.kind = AccessoryKind::Interior;
        interior.wheel = None;
        interior.model.variant = Some("s_h_std_eu".to_owned());
        let locators = [
            locator("interior_s", [-0.7, 2.7, -1.9]),
            locator("ext_interior", [-0.683, 2.705, -1.862]),
        ];
        let mut warnings = Vec::new();
        interior.model.exterior_uk = Some("/uk.pmd".to_owned());
        interior.model.variant_uk = Some("s_h_std_uk".to_owned());
        let placements = interior_placements(&[&interior], &locators, Drive::Left, &mut warnings);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(placements.len(), 1);
        assert_eq!(placements[0].transform.translation, [-0.683, 2.705, -1.862]);
        assert_eq!(placements[0].variant.as_deref(), Some("s_h_std_eu"));

        let uk = interior_placements(&[&interior], &locators, Drive::Right, &mut warnings);
        assert_eq!(uk[0].model, "/uk.pmd");
        assert_eq!(uk[0].variant.as_deref(), Some("s_h_std_uk"));

        let none = interior_placements(&[&interior], &locators[..1], Drive::Left, &mut warnings);
        assert!(none.is_empty());
        assert_eq!(warnings.len(), 1, "a missing locator is reported");
    }

    #[test]
    fn composes_parent_and_child_transforms() {
        let half = std::f32::consts::FRAC_1_SQRT_2;
        // Parent at (1, 2, 3), turned 90 degrees about +y; child 1 m along
        // the parent's +x lands 1 m along world -z.
        let parent = Transform {
            translation: [1.0, 2.0, 3.0],
            rotation: [0.0, half, 0.0, half],
            scale: 1.0,
        };
        let child = Transform {
            translation: [1.0, 0.0, 0.0],
            ..Transform::IDENTITY
        };
        let world = parent.compose(&child);
        let close = |a: [f32; 3], b: [f32; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-5);
        assert!(
            close(world.translation, [1.0, 2.0, 2.0]),
            "{:?}",
            world.translation
        );
        assert_eq!(Transform::IDENTITY.compose(&child), child);
        let twice = parent.compose(&parent);
        // Two 90 degree turns are 180 degrees about +y: (0, 1, 0, 0).
        assert!(
            (twice.rotation[1].abs() - 1.0).abs() < 1e-5,
            "{:?}",
            twice.rotation
        );
    }

    #[test]
    fn rotates_by_quaternion() {
        // 90 degrees about +y maps +x to -z.
        let half = std::f32::consts::FRAC_1_SQRT_2;
        let v = rotate([0.0, half, 0.0, half], [1.0, 0.0, 0.0]);
        assert!(v[0].abs() < 1e-6 && (v[2] + 1.0).abs() < 1e-6, "{v:?}");
    }

    #[test]
    fn transforms_boxes() {
        let aabb = Aabb {
            min: [-0.25, 0.0, 0.0],
            max: [0.25, 0.5, 1.0],
        };
        let transform = Transform {
            translation: [1.0, 2.0, 3.0],
            ..Transform::IDENTITY
        };
        let world = transform_aabb(&aabb, &transform);
        assert_eq!(world.min, [0.75, 2.0, 3.0]);
        assert_eq!(world.max, [1.25, 2.5, 4.0]);
    }
}
