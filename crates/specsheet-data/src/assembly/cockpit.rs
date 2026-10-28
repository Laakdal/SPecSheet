//! Interior accessories on the cockpit (finding F10): the steering wheel,
//! curtains, cab toys, glass sets and the like have only an
//! `interior_model`, and their locators live on the interior's full cockpit
//! model, not on the cabin. They are placed there and seen from the
//! driver's seat.

use std::collections::{BTreeMap, BTreeSet};

use specsheet_core::{Accessory, AccessoryKind, SlotId, TruckCatalog};
use specsheet_formats::pmg::Locator;

use super::{Drive, Placement, Transform};
use crate::fs::GameFs;
use crate::model::ModelCache;
use crate::slots::locators_by_slot;

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
    let t = parent.compose(&Transform::from_locator(locator));
    let [x, y, z, w] = t.rotation;
    Locator {
        name: locator.name.clone(),
        position: t.translation,
        scale: t.scale,
        rotation_wxyz: [w, x, y, z],
        hookup: locator.hookup.clone(),
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
/// same longest-prefix rule as the cabin's (F4).
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
/// locators. A slot without one (carpets) sits at the cockpit's origin,
/// the driver's eye point: such models are authored in the cockpit's frame
/// (Hypothesis). Right-hand drive uses `interior_model_uk` when given.
pub(super) fn interior_part_placements(
    selected: &[&Accessory],
    by_slot: &BTreeMap<SlotId, Vec<&Locator>>,
    cockpit: Option<&Transform>,
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
        let transforms: Vec<Transform> = match (by_slot.get(&accessory.slot), cockpit) {
            (Some(locators), _) => locators
                .iter()
                .map(|l| Transform::from_locator(l))
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
    use specsheet_core::{AccessoryId, ModelRef};

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
            Drive::Right,
            &mut warnings,
        );
        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].model, "/carpet_uk.pmd");
        assert_eq!(placed[0].transform, eye);
        let none = interior_part_placements(&[&carpet], &by_slot, None, Drive::Left, &mut warnings);
        assert!(none.is_empty());
        assert_eq!(warnings.len(), 1, "no cockpit at all is reported");
    }
}
