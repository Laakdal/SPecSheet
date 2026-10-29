//! Accessory units: class to kind, model references, wheel parts and the
//! right-hand-drive twins of interiors (F3, F9, F10).

use std::collections::HashMap;

use specsheet_core::{AccessoryId, SlotId};
use specsheet_formats::sii::Unit;
use specsheet_truck::{slots, Accessory, AccessoryKind, ModelRef, WheelPart};

use super::paint::{color3, paint_job_from_unit};
use super::{item_label, owned};

/// Gives every interior the exterior-view model of its right-hand-drive
/// twin as `exterior_uk` / `variant_uk`, and links the two as each other's
/// `drive_twin` (finding F10). The twin lives in the
/// same directory and its file name is the interior's with one extra `uk`
/// word (`uk_a.sii`, `a_uk.sii`, `standard_uk_low.sii` for `standard_low`).
/// This pairs all 219 interiors of 1.60 one-to-one.
pub(super) fn pair_uk_interiors(accessories: &mut [Accessory]) {
    let key = |path: &str| -> Option<(String, Vec<String>)> {
        let (dir, file) = path.rsplit_once('/')?;
        let stem = file.strip_suffix(".sii")?;
        Some((dir.to_owned(), stem.split('_').map(str::to_owned).collect()))
    };
    // EU key (directory, words without one `uk`) -> UK twin's model and variant.
    type Key = (String, Vec<String>);
    let mut twins: HashMap<Key, (Option<String>, Option<String>, AccessoryId)> = HashMap::new();
    for accessory in accessories
        .iter()
        .filter(|a| a.kind == AccessoryKind::Interior)
    {
        let Some((dir, words)) = key(&accessory.def_path) else {
            continue;
        };
        for (index, _) in words.iter().enumerate().filter(|(_, w)| *w == "uk") {
            let mut eu_words = words.clone();
            eu_words.remove(index);
            twins.insert(
                (dir.clone(), eu_words),
                (
                    accessory.model.exterior.clone(),
                    accessory.model.variant.clone(),
                    accessory.id.clone(),
                ),
            );
        }
    }
    for accessory in accessories
        .iter_mut()
        .filter(|a| a.kind == AccessoryKind::Interior)
    {
        let Some(twin) = key(&accessory.def_path).and_then(|k| twins.get(&k)) else {
            continue;
        };
        accessory.model.exterior_uk = twin.0.clone();
        accessory.model.variant_uk = twin.1.clone();
        accessory.drive_twin = Some(twin.2.clone());
    }
    // The UK units point back at their EU twin.
    let back: HashMap<AccessoryId, AccessoryId> = accessories
        .iter()
        .filter_map(|a| Some((a.drive_twin.clone()?, a.id.clone())))
        .collect();
    for accessory in accessories.iter_mut() {
        if let Some(eu) = back.get(&accessory.id) {
            accessory.drive_twin = Some(eu.clone());
        }
    }
}

pub(super) fn kind_for_class(class: &str) -> Option<AccessoryKind> {
    Some(match class {
        "accessory_chassis_data" => AccessoryKind::Chassis,
        "accessory_cabin_data" => AccessoryKind::Cabin,
        "accessory_interior_data" => AccessoryKind::Interior,
        "accessory_engine_data" => AccessoryKind::Engine,
        "accessory_transmission_data" => AccessoryKind::Transmission,
        "accessory_head_lights_data" => AccessoryKind::HeadLights,
        "accessory_paint_job_data" => AccessoryKind::PaintJob,
        "accessory_wheel_data" | "accessory_rim_data" => AccessoryKind::Wheel,
        c if c.starts_with("accessory_addon") || c == "accessory_horn_addon_data" => {
            AccessoryKind::Addon
        }
        _ => return None,
    })
}

pub(super) fn accessory_from_unit(unit: &Unit, slot: &SlotId, path: &str) -> Option<Accessory> {
    let kind = kind_for_class(&unit.class)?;
    let model = if kind == AccessoryKind::Interior {
        // The full cockpit (`model`) is for the interior camera; the
        // exterior view uses the light `ext_model`, which all 438 interior
        // units carry (`research/evidence/interior-survey.txt`, E1).
        ModelRef {
            exterior: unit.get_str("ext_model").map(str::to_owned),
            exterior_uk: None,
            look: unit.get_str("ext_look").map(str::to_owned),
            variant: unit.get_str("ext_variant").map(str::to_owned),
            variant_uk: None,
            cockpit: unit.get_str("model").map(str::to_owned),
            cockpit_look: unit.get_str("look").map(str::to_owned),
            cockpit_variant: unit.get_str("variant").map(str::to_owned),
            cockpit_uk: None,
            cockpit_animated: unit.get_str("animated_model").map(str::to_owned),
            ..ModelRef::default()
        }
    } else if kind.is_base() || kind == AccessoryKind::Wheel {
        ModelRef {
            exterior: unit
                .get_str("detail_model")
                .or_else(|| unit.get_str("model"))
                .map(str::to_owned),
            exterior_uk: None,
            look: unit.get_str("look").map(str::to_owned),
            variant: unit.get_str("variant").map(str::to_owned),
            variant_uk: unit.get_str("variant_uk").map(str::to_owned),
            windows: unit.get_str("windows_model").map(str::to_owned),
            wipers: unit.get_str("wiper_model").map(str::to_owned),
            wipers_uk: unit.get_str("wiper_model_uk").map(str::to_owned),
            ui_shadow: unit.get_str("ui_shadow").map(str::to_owned),
            ..ModelRef::default()
        }
    } else {
        ModelRef {
            exterior: unit.get_str("exterior_model").map(str::to_owned),
            exterior_uk: unit.get_str("exterior_model_uk").map(str::to_owned),
            look: unit.get_str("look").map(str::to_owned),
            variant: unit.get_str("variant").map(str::to_owned),
            variant_uk: None,
            // Interior accessories: the model seen from the driver's seat.
            cockpit: unit.get_str("interior_model").map(str::to_owned),
            cockpit_look: unit.get_str("look").map(str::to_owned),
            cockpit_variant: unit.get_str("variant").map(str::to_owned),
            cockpit_uk: unit.get_str("interior_model_uk").map(str::to_owned),
            ..ModelRef::default()
        }
    };
    // Cabins list required addon slots in `require[]`.
    let requires = if kind == AccessoryKind::Cabin {
        unit.array_str("require")
            .into_iter()
            .map(SlotId::from)
            .collect()
    } else {
        Vec::new()
    };
    Some(Accessory {
        id: AccessoryId::new(unit.name.clone()),
        slot: if kind == AccessoryKind::PaintJob {
            SlotId::from(slots::PAINT_JOB)
        } else {
            slot.clone()
        },
        kind,
        def_path: path.to_owned(),
        // A few definitions have no `name`; their item id (`s770` in
        // `s770.scania.s_2016.badge`) is the most readable fallback.
        name: unit
            .get_str("name")
            .map(str::to_owned)
            .unwrap_or_else(|| item_label(&unit.name)),
        icon: unit.get_str("icon").map(str::to_owned),
        price: unit.get_i64("price"),
        unlock_level: unit.get_i64("unlock"),
        model,
        suitable_for: unit
            .array_str("suitable_for")
            .into_iter()
            .map(AccessoryId::from)
            .collect(),
        conflict_with: unit
            .array_str("conflict_with")
            .into_iter()
            .map(AccessoryId::from)
            .collect(),
        defaults: owned(unit.array_str("defaults")),
        requires,
        base_color: unit.get("base_color").and_then(color3),
        wheel: wheel_part(unit),
        left_hand_traffic: kind == AccessoryKind::Interior
            && unit.get_bool("left_hand_traffic").unwrap_or(false),
        drive_twin: None,
        paint: (kind == AccessoryKind::PaintJob).then(|| Box::new(paint_job_from_unit(unit))),
    })
}

/// Wheel role by unit class (finding F9).
fn wheel_part(unit: &Unit) -> Option<WheelPart> {
    match unit.class.as_str() {
        "accessory_wheel_data" => Some(WheelPart::Tire),
        "accessory_rim_data" => Some(WheelPart::Rim {
            tire_shift: unit.get_f32("tire_shift"),
        }),
        _ => None,
    }
}
