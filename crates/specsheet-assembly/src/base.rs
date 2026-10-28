//! Base models (chassis, cabin): placement at the origin, visible locators
//! and the slot availability they give addons.

use std::collections::BTreeSet;

use specsheet_core::Aabb;
use specsheet_core::SlotId;
use specsheet_formats::pmg::Locator;
use specsheet_truck::{Configuration, SlotAvailability, TruckCatalog};

use super::{Drive, Placement, Transform};
use crate::slots::locators_by_slot;
use specsheet_data::model::ModelCache;
use specsheet_data::GameFs;

/// Base models (chassis, cabin) of a configuration: their placements,
/// visible locators and bounding boxes.
pub(super) struct BaseLayout<'a> {
    pub(super) placements: Vec<Placement>,
    pub(super) locators: Vec<Locator>,
    pub(super) boxes: Vec<(Aabb, &'a specsheet_truck::Accessory)>,
    pub(super) warnings: Vec<String>,
}

pub(super) fn base_layout<'a>(
    fs: &GameFs,
    cache: &ModelCache,
    catalog: &'a TruckCatalog,
    config: &Configuration,
    drive: Drive,
) -> BaseLayout<'a> {
    let mut layout = BaseLayout {
        placements: Vec::new(),
        locators: Vec::new(),
        boxes: Vec::new(),
        warnings: Vec::new(),
    };
    let bases = config
        .selections()
        .values()
        .filter_map(|id| catalog.accessory(id))
        .filter(|a| a.kind.is_base());
    for accessory in bases {
        let Some(path) = accessory.model.exterior.as_deref() else {
            continue;
        };
        let variant = match drive {
            Drive::Right => accessory
                .model
                .variant_uk
                .as_ref()
                .or(accessory.model.variant.as_ref()),
            Drive::Left => accessory.model.variant.as_ref(),
        };
        match cache.load(fs, path) {
            Ok(model) => {
                let index = model.variant_index(variant.map(String::as_str));
                layout
                    .locators
                    .extend(model.visible_locators(index).cloned());
                layout.placements.push(Placement {
                    slot: accessory.slot.clone(),
                    accessory: accessory.id.clone(),
                    model: path.to_owned(),
                    look: accessory.model.look.clone(),
                    variant: variant.cloned(),
                    transform: Transform::IDENTITY,
                });
                layout.boxes.push((model.pmg.aabb, accessory));
            }
            Err(error) => layout.warnings.push(format!("{}: {error}", accessory.id)),
        }
    }
    layout
}

pub(super) fn addon_slots(catalog: &TruckCatalog) -> BTreeSet<SlotId> {
    catalog
        .accessories()
        .filter(|a| a.needs_exterior_locator())
        .map(|a| a.slot.clone())
        .collect()
}

/// Which addon slots the base models of `config` can host. Cheaper than a
/// full [`assemble`]; used while the rules repair a configuration.
pub fn slot_availability(
    fs: &GameFs,
    cache: &ModelCache,
    catalog: &TruckCatalog,
    config: &Configuration,
    drive: Drive,
) -> SlotAvailability {
    let base = base_layout(fs, cache, catalog, config, drive);
    SlotAvailability {
        available: locators_by_slot(&addon_slots(catalog), base.locators.iter())
            .into_keys()
            .collect(),
    }
}
