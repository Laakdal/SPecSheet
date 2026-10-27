//! Builds `tuning_core` catalogs from `/def/vehicle/truck/<truck>/` and the
//! preset configurations from `/def/vehicle/truck_desktop/` (findings F3,
//! F6 in `docs/REVERSE_ENGINEERING.md`).

use scs_formats::sii::Unit;
use tuning_core::{
    slots, Accessory, AccessoryId, AccessoryKind, Configuration, ModelRef, SlotId, TruckCatalog,
    TruckId, WheelPart,
};

use crate::error::{GameDataError, Result};
use crate::fs::GameFs;

const TRUCK_DEFS: &str = "/def/vehicle/truck";
const PRESET_DEFS: &str = "/def/vehicle/truck_desktop";
/// Directory holding addon slots: `accessory/<slot>/<item>.sii`.
const ADDON_DIR: &str = "accessory";
/// Shared wheel part groups under `/def/vehicle/<group>/`, used by every
/// truck (`t_wheel` is for trailers and therefore excluded).
const WHEEL_GROUPS: &[&str] = &[
    "f_tire", "f_disc", "f_rim", "f_hub", "f_nuts", "f_cover", "f_wheel", "r_tire", "r_disc",
    "r_rim", "r_hub", "r_nuts", "r_cover", "r_wheel",
];

/// A truck model available in the installation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TruckSummary {
    pub id: TruckId,
    /// Localisation keys of brand and series, e.g. `@@brand_scania@@`.
    pub info: Vec<String>,
}

/// A definition that could not be turned into an accessory. Loading
/// continues; these are reported so broken data is visible, not fatal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadWarning {
    pub path: String,
    pub message: String,
}

/// Every truck with a `data.sii`, sorted by id.
pub fn list_trucks(fs: &GameFs) -> Result<Vec<TruckSummary>> {
    let mut trucks = Vec::new();
    for dir in fs.list_dir(TRUCK_DEFS)?.directories {
        let data_path = format!("{TRUCK_DEFS}/{dir}/data.sii");
        if !fs.exists(&data_path) {
            continue;
        }
        let doc = fs.read_sii(&data_path)?;
        let info = doc
            .units_of_class("accessory_truck_data")
            .next()
            .map(|unit| {
                unit.array_str("info")
                    .into_iter()
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        trucks.push(TruckSummary {
            id: TruckId::new(dir),
            info,
        });
    }
    trucks.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(trucks)
}

/// Loads every accessory of `truck`.
pub fn load_catalog(fs: &GameFs, truck: &TruckId) -> Result<(TruckCatalog, Vec<LoadWarning>)> {
    let root = format!("{TRUCK_DEFS}/{truck}");
    let data_path = format!("{root}/data.sii");
    let data = fs.read_sii(&data_path)?;
    let truck_unit = data
        .units_of_class("accessory_truck_data")
        .next()
        .ok_or_else(|| GameDataError::Definition {
            path: data_path.clone(),
            message: "no accessory_truck_data unit".to_owned(),
        })?;
    let info = owned(truck_unit.array_str("info"));
    let required_slots = truck_unit
        .array_str("require")
        .into_iter()
        .map(SlotId::from)
        .collect();
    let fallbacks = truck_unit
        .array_str("fallback")
        .into_iter()
        .filter_map(|entry| resolve_fallback(fs, &root, entry))
        .collect();

    let mut accessories = Vec::new();
    let mut warnings = Vec::new();
    for group in WHEEL_GROUPS {
        let dir = format!("/def/vehicle/{group}");
        let slot = SlotId::from(*group);
        let Ok(listing) = fs.list_dir(&dir) else {
            continue;
        };
        for file in listing.files.iter().filter(|f| f.ends_with(".sii")) {
            let path = format!("{dir}/{file}");
            match fs.read_sii(&path) {
                Ok(doc) => accessories.extend(
                    doc.units
                        .iter()
                        .filter_map(|unit| accessory_from_unit(unit, &slot, &path)),
                ),
                Err(error) => warnings.push(LoadWarning {
                    path,
                    message: error.to_string(),
                }),
            }
        }
    }
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let listing = fs.list_dir(&dir)?;
        stack.extend(listing.directories.iter().map(|d| format!("{dir}/{d}")));
        let Some(slot) = slot_for_dir(&root, &dir) else {
            continue;
        };
        for file in listing.files.iter().filter(|f| f.ends_with(".sii")) {
            let path = format!("{dir}/{file}");
            match fs.read_sii(&path) {
                Ok(doc) => accessories.extend(
                    doc.units
                        .iter()
                        .filter_map(|unit| accessory_from_unit(unit, &slot, &path)),
                ),
                Err(error) => warnings.push(LoadWarning {
                    path,
                    message: error.to_string(),
                }),
            }
        }
    }

    Ok((
        TruckCatalog::new(truck.clone(), info, required_slots, fallbacks, accessories),
        warnings,
    ))
}

/// The game's preset configuration for `truck`, if one exists. Selections
/// whose definition is not in the catalog are dropped and reported.
pub fn load_preset(
    fs: &GameFs,
    catalog: &TruckCatalog,
) -> Result<Option<(Configuration, Vec<LoadWarning>)>> {
    let data_path = format!("{TRUCK_DEFS}/{}/data.sii", catalog.truck);
    for file in fs.list_dir(PRESET_DEFS)?.files {
        let doc = fs.read_sii(&format!("{PRESET_DEFS}/{file}"))?;
        let paths: Vec<&str> = doc
            .units
            .iter()
            .filter(|u| u.class.starts_with("vehicle_") && u.class.ends_with("accessory"))
            .filter_map(|u| u.get_str("data_path"))
            .collect();
        if !paths.iter().any(|p| p.eq_ignore_ascii_case(&data_path)) {
            continue;
        }
        let preset_path = format!("{PRESET_DEFS}/{file}");
        let mut selections = Vec::new();
        let mut warnings = Vec::new();
        let paint_color = doc
            .units_of_class("vehicle_paint_job_accessory")
            .find_map(|u| u.get("base_color").and_then(|v| v.as_floats()))
            .and_then(|v| <[f32; 3]>::try_from(v.get(..3)?).ok());
        // The truck's own `data.sii` is listed too; it is not an accessory.
        for path in paths
            .into_iter()
            .filter(|p| !p.eq_ignore_ascii_case(&data_path))
        {
            match catalog.by_def_path(path) {
                Some(a) => selections.push((a.slot.clone(), a.id.clone())),
                None => warnings.push(LoadWarning {
                    path: preset_path.clone(),
                    message: format!("selection {path} is not in the catalog"),
                }),
            }
        }
        let mut config = Configuration::from_selections(catalog.truck.clone(), selections);
        config.set_paint_color(paint_color);
        return Ok(Some((config, warnings)));
    }
    Ok(None)
}

/// The slot a definition directory provides accessories for:
/// `<root>/accessory/<slot>` for addons, `<root>/<group>` otherwise.
fn slot_for_dir(root: &str, dir: &str) -> Option<SlotId> {
    let relative = dir.strip_prefix(root)?.trim_start_matches('/');
    let mut parts = relative.split('/');
    match (parts.next(), parts.next()) {
        (Some(ADDON_DIR), Some(slot)) => Some(SlotId::from(slot)),
        (Some(ADDON_DIR), None) | (Some(""), _) | (None, _) => None,
        (Some(group), _) => Some(SlotId::from(group)),
    }
}

/// `fallback[]` entries are `"<group>|<file>"`, relative to the truck
/// directory (`chassis|4x2.sii`, `accessory|f_light_chs/shape2_blk.sii`) or,
/// for shared parts such as tyres, to `/def/vehicle/<group>/`.
fn resolve_fallback(fs: &GameFs, root: &str, entry: &str) -> Option<String> {
    let (group, file) = entry.split_once('|')?;
    let local = format!("{root}/{group}/{file}");
    if fs.exists(&local) {
        return Some(local);
    }
    let shared = format!("/def/vehicle/{group}/{file}");
    fs.exists(&shared).then_some(shared)
}

fn kind_for_class(class: &str) -> Option<AccessoryKind> {
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

fn accessory_from_unit(unit: &Unit, slot: &SlotId, path: &str) -> Option<Accessory> {
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
        }
    } else {
        ModelRef {
            exterior: unit.get_str("exterior_model").map(str::to_owned),
            exterior_uk: unit.get_str("exterior_model_uk").map(str::to_owned),
            look: unit.get_str("look").map(str::to_owned),
            variant: unit.get_str("variant").map(str::to_owned),
            variant_uk: None,
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
        base_color: unit
            .get("base_color")
            .and_then(|v| v.as_floats())
            .and_then(|v| <[f32; 3]>::try_from(v.get(..3)?).ok()),
        wheel: wheel_part(unit),
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

fn item_label(unit_name: &str) -> String {
    let item = unit_name.split('.').next().unwrap_or(unit_name);
    item.replace('_', " ").to_uppercase()
}

fn owned(items: Vec<&str>) -> Vec<String> {
    items.into_iter().map(str::to_owned).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_directories_map_to_slots() {
        let root = "/def/vehicle/truck/scania.s_2016";
        assert_eq!(
            slot_for_dir(root, "/def/vehicle/truck/scania.s_2016/accessory/s_mirror"),
            Some(SlotId::from("s_mirror"))
        );
        assert_eq!(
            slot_for_dir(root, "/def/vehicle/truck/scania.s_2016/cabin"),
            Some(SlotId::from("cabin"))
        );
        assert_eq!(
            slot_for_dir(root, "/def/vehicle/truck/scania.s_2016/accessory"),
            None
        );
        assert_eq!(slot_for_dir(root, root), None);
    }

    #[test]
    fn addon_classes_are_addons() {
        for class in [
            "accessory_addon_data",
            "accessory_addon_painted_data",
            "accessory_addon_int_data",
            "accessory_horn_addon_data",
        ] {
            assert_eq!(kind_for_class(class), Some(AccessoryKind::Addon), "{class}");
        }
        assert_eq!(kind_for_class("simple_paint_job_data"), None);
        assert_eq!(kind_for_class("physics_patch_data"), None);
    }
}
