//! Builds `specsheet_truck` catalogs from `/def/vehicle/truck/<truck>/` and the
//! preset configurations from `/def/vehicle/truck_desktop/` (findings F3,
//! F6, F15 in `docs/REVERSE_ENGINEERING.md`).

use std::collections::HashMap;

use specsheet_core::{SlotId, TruckId};
use specsheet_truck::{Hookup, TruckCatalog};

use crate::error::{GameDataError, Result};
use crate::fs::GameFs;

mod accessory;
mod hookup;
mod paint;
mod preset;
#[cfg(test)]
mod tests;

use accessory::{accessory_from_unit, pair_uk_interiors};
use hookup::load_hookups;
use paint::{attach_paint_parts, raw_part, RawPart, PAINT_PARTS_DIR};
pub use preset::{load_preset, load_vehicle_config};

const TRUCK_DEFS: &str = "/def/vehicle/truck";
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
    /// The truck's directory name, e.g. `scania.s_2016`.
    pub id: TruckId,
    /// Localisation keys of brand and series, e.g. `@@brand_scania@@`.
    pub info: Vec<String>,
}

/// A definition that could not be turned into an accessory. Loading
/// continues; these are reported so broken data is visible, not fatal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadWarning {
    /// The file it comes from.
    pub path: String,
    /// What was wrong.
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
    // Per-part paint units by the file stem they are stored under.
    let mut paint_parts: HashMap<String, Vec<RawPart>> = HashMap::new();
    let paint_parts_dir = format!("{root}/{PAINT_PARTS_DIR}");
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let listing = fs.list_dir(&dir)?;
        stack.extend(listing.directories.iter().map(|d| format!("{dir}/{d}")));
        let Some(slot) = slot_for_dir(&root, &dir) else {
            continue;
        };
        let parts_dir = dir.eq_ignore_ascii_case(&paint_parts_dir);
        for file in listing.files.iter().filter(|f| f.ends_with(".sii")) {
            let path = format!("{dir}/{file}");
            match fs.read_sii(&path) {
                Ok(doc) if parts_dir => {
                    let stem = file.trim_end_matches(".sii").to_ascii_lowercase();
                    paint_parts
                        .entry(stem)
                        .or_default()
                        .extend(doc.units.iter().filter_map(raw_part));
                }
                // The rarer per-accessory files
                // (`accessory/<slot>/paint_job/*.sii`) hold only
                // `simple_paint_job_data` and fall out here (F15).
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

    pair_uk_interiors(&mut accessories);
    attach_paint_parts(&mut accessories, truck, &paint_parts);
    let catalog = TruckCatalog::new(truck.clone(), info, required_slots, fallbacks, accessories);
    // Only hookups that fit one of this truck's slots are relevant.
    let slots = catalog.slots();
    let hookups: Vec<Hookup> = load_hookups(fs, &mut warnings)?
        .into_iter()
        .filter(|h| h.hosts.iter().any(|host| slots.contains(host)))
        .collect();
    Ok((catalog.with_hookups(hookups), warnings))
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

fn item_label(unit_name: &str) -> String {
    let item = unit_name.split('.').next().unwrap_or(unit_name);
    item.replace('_', " ").to_uppercase()
}

fn owned(items: Vec<&str>) -> Vec<String> {
    items.into_iter().map(str::to_owned).collect()
}
