//! The addon hookup catalogue (F11).

use specsheet_core::{AccessoryId, SlotId};
use specsheet_formats::sii::Unit;
use specsheet_truck::Hookup;

use super::{item_label, LoadWarning};
use crate::error::Result;
use crate::fs::GameFs;

/// Hookup catalogue: `addon_hookup_storage.sii` plus one file per DLC,
/// each including `addon_hookups/*.sui` (finding F11).
const HOOKUP_DIR: &str = "/def/vehicle";
const HOOKUP_STORAGE: &str = "addon_hookup_storage";

/// Every hookup unit of the installation.
pub(super) fn load_hookups(fs: &GameFs, warnings: &mut Vec<LoadWarning>) -> Result<Vec<Hookup>> {
    let mut hookups = Vec::new();
    for file in fs.list_dir(HOOKUP_DIR)?.files {
        if !(file.starts_with(HOOKUP_STORAGE) && file.ends_with(".sii")) {
            continue;
        }
        let path = format!("{HOOKUP_DIR}/{file}");
        match fs.read_sii(&path) {
            Ok(doc) => hookups.extend(doc.units.iter().filter_map(hookup_from_unit)),
            Err(error) => warnings.push(LoadWarning {
                path,
                message: error.to_string(),
            }),
        }
    }
    Ok(hookups)
}

/// `accessory_hookup_data`, `accessory_horn_hookup_data`,
/// `accessory_hookup_int_data`, `accessory_hookup_int_ui_data`.
fn hookup_from_unit(unit: &Unit) -> Option<Hookup> {
    if !(unit.class.starts_with("accessory_") && unit.class.contains("hookup")) {
        return None;
    }
    Some(Hookup {
        id: AccessoryId::new(unit.name.clone()),
        name: unit
            .get_str("name")
            .map(str::to_owned)
            .unwrap_or_else(|| item_label(&unit.name)),
        icon: unit.get_str("icon").map(str::to_owned),
        price: unit.get_i64("price"),
        unlock_level: unit.get_i64("unlock"),
        model: unit
            .get_str("model")
            .or_else(|| unit.get_str("exterior_model"))
            .map(str::to_owned),
        hosts: unit
            .array_str("suitable_for")
            .into_iter()
            .map(SlotId::from)
            .collect(),
        interior: unit.class.contains("_int_"),
    })
}
