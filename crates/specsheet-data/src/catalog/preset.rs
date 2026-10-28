//! Vehicle configurations: the game's presets and other `vehicle` files (F6).

use specsheet_core::AccessoryId;
use specsheet_formats::sii::{SiiDocument, Unit};
use specsheet_truck::{Configuration, TruckCatalog};

use super::paint::paint_override;
use super::{LoadWarning, TRUCK_DEFS};
use crate::error::Result;
use crate::fs::GameFs;

const PRESET_DEFS: &str = "/def/vehicle/truck_desktop";

/// The game's preset configuration for `truck`, if one exists. Selections
/// whose definition is not in the catalog are dropped and reported.
pub fn load_preset(
    fs: &GameFs,
    catalog: &TruckCatalog,
) -> Result<Option<(Configuration, Vec<LoadWarning>)>> {
    let data_path = format!("{TRUCK_DEFS}/{}/data.sii", catalog.truck);
    for file in fs.list_dir(PRESET_DEFS)?.files {
        let path = format!("{PRESET_DEFS}/{file}");
        let doc = fs.read_sii(&path)?;
        let for_truck = vehicle_units(&doc)
            .filter_map(|u| u.get_str("data_path"))
            .any(|p| p.eq_ignore_ascii_case(&data_path));
        if for_truck {
            return Ok(Some(config_from_vehicle(&doc, catalog, &path)));
        }
    }
    Ok(None)
}

/// Reads any vehicle configuration file (`vehicle` + `vehicle_*accessory`
/// units, as in `/def/vehicle/truck_desktop`, `truck_company`, `truck_used`)
/// for the truck of `catalog`.
pub fn load_vehicle_config(
    fs: &GameFs,
    catalog: &TruckCatalog,
    path: &str,
) -> Result<(Configuration, Vec<LoadWarning>)> {
    Ok(config_from_vehicle(&fs.read_sii(path)?, catalog, path))
}

fn vehicle_units(doc: &SiiDocument) -> impl Iterator<Item = &Unit> {
    doc.units
        .iter()
        .filter(|u| u.class.starts_with("vehicle_") && u.class.ends_with("accessory"))
}

pub(super) fn config_from_vehicle(
    doc: &SiiDocument,
    catalog: &TruckCatalog,
    source: &str,
) -> (Configuration, Vec<LoadWarning>) {
    let data_path = format!("{TRUCK_DEFS}/{}/data.sii", catalog.truck);
    let mut selections = Vec::new();
    let mut hookups = Vec::new();
    let mut warnings = Vec::new();
    let warn = |warnings: &mut Vec<LoadWarning>, message: String| {
        warnings.push(LoadWarning {
            path: source.to_owned(),
            message,
        })
    };
    for unit in vehicle_units(doc) {
        let Some(path) = unit.get_str("data_path") else {
            continue;
        };
        // The truck's own `data.sii` is listed too; it is not an accessory.
        if path.eq_ignore_ascii_case(&data_path) {
            continue;
        }
        let Some(accessory) = catalog.by_def_path(path) else {
            warn(
                &mut warnings,
                format!("selection {path} is not in the catalog"),
            );
            continue;
        };
        selections.push((accessory.slot.clone(), accessory.id.clone()));
        // Hookups mounted on this accessory: `slot_name[i]` is a position
        // (`slot_NN` locator of its model), `slot_hookup[i]` the hookup unit.
        let names = unit.array_str("slot_name");
        let units = unit.array_str("slot_hookup");
        if names.len() != units.len() {
            warn(
                &mut warnings,
                format!(
                    "{path}: {} slot names for {} hookups",
                    names.len(),
                    units.len()
                ),
            );
        }
        for (position, hookup) in names.into_iter().zip(units) {
            hookups.push((
                accessory.slot.clone(),
                position.to_owned(),
                AccessoryId::from(hookup),
            ));
        }
    }
    let paint = doc
        .units_of_class("vehicle_paint_job_accessory")
        .next()
        .map(|unit| paint_override(unit, |message| warn(&mut warnings, message)))
        .unwrap_or_default();
    let mut config =
        Configuration::from_selections(catalog.truck.clone(), selections).with_hookups(hookups);
    config.set_paint_override(paint);
    (config, warnings)
}
