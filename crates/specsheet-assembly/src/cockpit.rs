//! Interior accessories on the cockpit (finding F10): the steering wheel,
//! curtains, cab toys, glass sets and the like have only an
//! `interior_model`, and their locators live on the interior's full cockpit
//! model, not on the cabin. They are placed there and seen from the
//! driver's seat.

use std::collections::{BTreeMap, BTreeSet};

use specsheet_core::SlotId;
use specsheet_formats::pmg::{Bone, Locator};
use specsheet_truck::{Accessory, AccessoryKind, TruckCatalog};

use super::{Drive, Placement, Transform};
use crate::slots::locators_by_slot;
use specsheet_data::model::ModelCache;
use specsheet_data::GameFs;

/// The cockpit's visible locators, moved into truck space.
pub(super) fn cockpit_locators(
    fs: &GameFs,
    cache: &ModelCache,
    cockpit: &Placement,
    warnings: &mut Vec<String>,
) -> Vec<Locator> {
    let model = match cache.load(fs, &cockpit.model) {
        Ok(model) => model,
        Err(error) => {
            warnings.push(format!("{}: {error}", cockpit.model));
            return Vec::new();
        }
    };
    let variant = model.variant_index(cockpit.variant.as_deref());
    model
        .visible_locators(variant)
        .map(|locator| to_truck_space(&cockpit.transform, locator))
        .collect()
}

fn to_truck_space(parent: &Transform, locator: &Locator) -> Locator {
    let t = parent.compose(&super::transform::from_locator(locator));
    let [x, y, z, w] = t.rotation;
    Locator {
        name: locator.name.clone(),
        position: t.translation,
        scale: t.scale,
        rotation_wxyz: [w, x, y, z],
        hookup: locator.hookup.clone(),
    }
}

/// The bone and the slot of the steering wheel (rea-steering-wheel.txt).
const STEERING_WHEEL: &str = "steering_w";

/// The steering wheel's pose in the cockpit's space, the game's way
/// (`research/evidence/rea-steering-wheel.txt`): from the `steering_w`
/// bone of the interior's animated model, else of the cockpit model. The
/// position is the bone's translation; the rotation is the bone's,
/// followed by a turn about the wheel's own y axis that cancels the bone's
/// heading (the heading of the inverse bone). The cockpit's `steering_w`
/// locators are not a consistent frame between trucks or drive sides.
pub(super) fn steering_pose(
    fs: &GameFs,
    cache: &ModelCache,
    models: &[&Placement],
) -> Option<Transform> {
    models.iter().find_map(|placement| {
        let model = cache.load(fs, &placement.model).ok()?;
        model.pmg.bone(STEERING_WHEEL).map(bone_pose)
    })
}

/// A bone's rest pose with its heading cancelled (see [`steering_pose`]).
fn bone_pose(bone: &Bone) -> Transform {
    let m = &bone.matrix;
    let rest = m[2][0].atan2(m[2][2]);
    let (s, c) = rest.sin_cos();
    // The bone's rotation times a turn by `rest` about y.
    let r = |i: usize| {
        [
            m[i][0] * c - m[i][2] * s,
            m[i][1],
            m[i][0] * s + m[i][2] * c,
        ]
    };
    Transform {
        translation: bone.translation(),
        rotation: quaternion([r(0), r(1), r(2)]),
        scale: 1.0,
    }
}

/// The unit quaternion `(x, y, z, w)` of a rotation matrix (rows, for
/// column vectors).
fn quaternion(m: [[f32; 3]; 3]) -> [f32; 4] {
    let trace = m[0][0] + m[1][1] + m[2][2];
    let q = if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        [
            (m[2][1] - m[1][2]) / s,
            (m[0][2] - m[2][0]) / s,
            (m[1][0] - m[0][1]) / s,
            0.25 * s,
        ]
    } else if m[0][0] > m[1][1] && m[0][0] > m[2][2] {
        let s = (1.0 + m[0][0] - m[1][1] - m[2][2]).sqrt() * 2.0;
        [
            0.25 * s,
            (m[0][1] + m[1][0]) / s,
            (m[0][2] + m[2][0]) / s,
            (m[2][1] - m[1][2]) / s,
        ]
    } else if m[1][1] > m[2][2] {
        let s = (1.0 + m[1][1] - m[0][0] - m[2][2]).sqrt() * 2.0;
        [
            (m[0][1] + m[1][0]) / s,
            0.25 * s,
            (m[1][2] + m[2][1]) / s,
            (m[0][2] - m[2][0]) / s,
        ]
    } else {
        let s = (1.0 + m[2][2] - m[0][0] - m[1][1]).sqrt() * 2.0;
        [
            (m[0][2] + m[2][0]) / s,
            (m[1][2] + m[2][1]) / s,
            0.25 * s,
            (m[1][0] - m[0][1]) / s,
        ]
    };
    let norm = q.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 1e-6 {
        q.map(|v| v / norm)
    } else {
        Transform::IDENTITY.rotation
    }
}

/// Interior-only accessories (an interior model, no exterior one) need the
/// cockpit's locators.
fn interior_only(accessory: &Accessory) -> bool {
    accessory.kind == AccessoryKind::Addon
        && accessory.model.exterior.is_none()
        && accessory.model.cockpit.is_some()
}

/// Slots of interior-only accessories: the slots the cockpit may host.
pub(super) fn interior_slots(catalog: &TruckCatalog) -> BTreeSet<SlotId> {
    catalog
        .accessories()
        .filter(|a| interior_only(a))
        .map(|a| a.slot.clone())
        .collect()
}

/// The cockpit locators of slots the cabin does not offer, matched with the
/// same longest-prefix rule as the cabin's (F4): the slots that get only an
/// interior pin.
pub(super) fn cockpit_slots<'a>(
    addon_slots: &BTreeSet<SlotId>,
    exterior: &BTreeMap<SlotId, Vec<&Locator>>,
    locators: &'a [Locator],
) -> BTreeMap<SlotId, Vec<&'a Locator>> {
    let mut by_slot = locators_by_slot(addon_slots, locators.iter());
    by_slot.retain(|slot, _| !exterior.contains_key(slot));
    by_slot
}

/// Places the selected interior-only accessories at their cockpit
/// locators (`by_slot`, matched over the cockpit's own locators, whether or
/// not the cabin has one for the slot). A slot without one (Scania
/// carpets) sits at the cockpit's origin, the driver's eye point: such
/// models are authored in the cockpit's frame (Hypothesis). Right-hand
/// drive uses `interior_model_uk` when given. The steering wheel takes
/// `steering`, its bone pose in the cockpit's space, when there is one.
pub(super) fn interior_part_placements(
    selected: &[&Accessory],
    by_slot: &BTreeMap<SlotId, Vec<&Locator>>,
    cockpit: Option<&Transform>,
    steering: Option<&Transform>,
    drive: Drive,
    warnings: &mut Vec<String>,
) -> Vec<Placement> {
    let mut placements = Vec::new();
    for accessory in selected.iter().filter(|a| interior_only(a)) {
        let model = match drive {
            Drive::Right => accessory
                .model
                .cockpit_uk
                .as_ref()
                .or(accessory.model.cockpit.as_ref()),
            Drive::Left => accessory.model.cockpit.as_ref(),
        };
        let Some(model) = model else {
            continue;
        };
        let wheel = accessory.slot.as_str().ends_with(STEERING_WHEEL);
        let transforms: Vec<Transform> = match (by_slot.get(&accessory.slot), cockpit) {
            (_, Some(eye)) if wheel && steering.is_some() => {
                steering.map(|pose| eye.compose(pose)).into_iter().collect()
            }
            (Some(locators), _) => locators
                .iter()
                .map(|l| super::transform::from_locator(l))
                .collect(),
            (None, Some(eye)) => vec![*eye],
            (None, None) => {
                warnings.push(format!(
                    "{}: no cockpit for slot {}",
                    accessory.id, accessory.slot
                ));
                continue;
            }
        };
        for transform in transforms {
            placements.push(Placement {
                slot: accessory.slot.clone(),
                accessory: accessory.id.clone(),
                model: model.clone(),
                look: accessory.model.cockpit_look.clone(),
                variant: accessory.model.cockpit_variant.clone(),
                transform,
            });
        }
    }
    placements
}

#[cfg(test)]
mod tests {
    use super::*;
    use specsheet_core::AccessoryId;
    use specsheet_truck::ModelRef;

    fn locator(name: &str, position: [f32; 3]) -> Locator {
        Locator {
            name: name.to_owned(),
            position,
            scale: 1.0,
            rotation_wxyz: [1.0, 0.0, 0.0, 0.0],
            hookup: None,
        }
    }

    fn addon(id: &str, slot: &str, model: ModelRef) -> Accessory {
        Accessory {
            id: AccessoryId::from(id),
            slot: SlotId::from(slot),
            kind: AccessoryKind::Addon,
            def_path: String::new(),
            name: id.to_owned(),
            icon: None,
            price: None,
            unlock_level: None,
            model,
            suitable_for: Vec::new(),
            conflict_with: Vec::new(),
            defaults: Vec::new(),
            requires: Vec::new(),
            base_color: None,
            wheel: None,
            left_hand_traffic: false,
            drive_twin: None,
            paint: None,
        }
    }

    #[test]
    fn cockpit_locators_move_to_the_eye_point() {
        let eye = Transform {
            translation: [-0.683, 2.705, -1.862],
            ..Transform::IDENTITY
        };
        let wheel = to_truck_space(&eye, &locator("steering_w", [0.0, -0.372, -0.555]));
        let [x, y, z] = wheel.position;
        assert!((x + 0.683).abs() < 1e-5 && (y - 2.333).abs() < 1e-5 && (z + 2.417).abs() < 1e-5);
    }

    /// The Scania 2016 animated cockpit's `steering_w` bone, rounded: its
    /// rotation turns about y as well as x, and the game cancels the turn
    /// (rea-steering-wheel.txt), leaving a 37° tilt about x.
    fn scania_wheel_bone() -> Bone {
        Bone {
            name: "steering_w".to_owned(),
            matrix: [
                [-0.0056, 0.0, 1.0, 0.001],
                [0.6019, 0.7985, 0.0034, -0.2746],
                [-0.7985, 0.6019, -0.0045, -0.4814],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    #[test]
    fn the_wheel_bone_gives_a_pure_tilt_about_x() {
        let pose = bone_pose(&scania_wheel_bone());
        assert_eq!(pose.translation, [0.001, -0.2746, -0.4814]);
        let [x, y, z, w] = pose.rotation;
        assert!(y.abs() < 0.005 && z.abs() < 0.005, "{:?}", pose.rotation);
        let tilt = 2.0 * x.atan2(w).to_degrees();
        assert!((tilt - 37.0).abs() < 0.5, "{tilt}");
    }

    #[test]
    fn the_steering_wheel_takes_the_bone_pose_over_its_locator() {
        let slots: BTreeSet<SlotId> = ["steering_w"].map(SlotId::from).into();
        // A locator turned another way, as the UK Scania cockpit's is.
        let mut turned = locator("steering_w", [0.0, -0.372, -0.555]);
        turned.rotation_wxyz = [0.894, -0.447, 0.0, 0.0];
        let cockpit = [turned];
        let by_slot = locators_by_slot(&slots, cockpit.iter());
        let wheel = addon(
            "std.t.steering_w",
            "steering_w",
            ModelRef {
                cockpit: Some("/steering.pmd".to_owned()),
                ..ModelRef::default()
            },
        );
        let eye = Transform {
            translation: [-0.683, 2.705, -1.862],
            ..Transform::IDENTITY
        };
        let pose = bone_pose(&scania_wheel_bone());
        let mut warnings = Vec::new();
        let placed = interior_part_placements(
            &[&wheel],
            &by_slot,
            Some(&eye),
            Some(&pose),
            Drive::Right,
            &mut warnings,
        );
        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].transform, eye.compose(&pose));
        // Without a bone, the locator.
        let placed = interior_part_placements(
            &[&wheel],
            &by_slot,
            Some(&eye),
            None,
            Drive::Left,
            &mut warnings,
        );
        assert_eq!(placed[0].transform.translation, [0.0, -0.372, -0.555]);
    }

    #[test]
    fn interior_only_parts_use_cockpit_slots_the_cabin_lacks() {
        let slots: BTreeSet<SlotId> = ["steering_w", "mirror"].map(SlotId::from).into();
        let cabin = [locator("mirror", [1.0, 2.0, -2.0])];
        let exterior = locators_by_slot(&slots, cabin.iter());
        let cockpit = [
            locator("steering_w", [-0.7, 2.3, -2.4]),
            // The cab's inner mirror shares the exterior slot's name.
            locator("mirror", [0.0, 2.6, -2.5]),
        ];
        let by_slot = cockpit_slots(&slots, &exterior, &cockpit);
        assert_eq!(
            by_slot.keys().map(SlotId::as_str).collect::<Vec<_>>(),
            ["steering_w"]
        );

        let wheel = addon(
            "golden.t.steering_w",
            "steering_w",
            ModelRef {
                cockpit: Some("/steering.pmd".to_owned()),
                cockpit_look: Some("Golden".to_owned()),
                ..ModelRef::default()
            },
        );
        let side_mirror = addon(
            "chrome.t.mirror",
            "mirror",
            ModelRef {
                exterior: Some("/mirror.pmd".to_owned()),
                ..ModelRef::default()
            },
        );
        let mut warnings = Vec::new();
        let placed = interior_part_placements(
            &[&wheel, &side_mirror],
            &by_slot,
            None,
            None,
            Drive::Left,
            &mut warnings,
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(placed.len(), 1, "exterior parts are not placed here");
        assert_eq!(placed[0].model, "/steering.pmd");
        assert_eq!(placed[0].look.as_deref(), Some("Golden"));
        assert_eq!(placed[0].transform.translation, [-0.7, 2.3, -2.4]);

        // A carpet has no locator: it sits at the eye point, UK model in RHD.
        let carpet = addon(
            "std.t.carpet",
            "carpet",
            ModelRef {
                cockpit: Some("/carpet.pmd".to_owned()),
                cockpit_uk: Some("/carpet_uk.pmd".to_owned()),
                ..ModelRef::default()
            },
        );
        let eye = Transform {
            translation: [-0.683, 2.705, -1.862],
            ..Transform::IDENTITY
        };
        let placed = interior_part_placements(
            &[&carpet],
            &by_slot,
            Some(&eye),
            None,
            Drive::Right,
            &mut warnings,
        );
        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].model, "/carpet_uk.pmd");
        assert_eq!(placed[0].transform, eye);
        let none =
            interior_part_placements(&[&carpet], &by_slot, None, None, Drive::Left, &mut warnings);
        assert!(none.is_empty());
        assert_eq!(warnings.len(), 1, "no cockpit at all is reported");
    }
}
