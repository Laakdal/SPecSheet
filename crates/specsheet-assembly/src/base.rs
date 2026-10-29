//! Base models (chassis, cabin): placement at the origin, visible locators
//! and the slot availability they give addons.

use std::collections::BTreeSet;

use specsheet_core::SlotId;
use specsheet_formats::pmg::Locator;
use specsheet_truck::{Configuration, ModelRef, SlotAvailability, TruckCatalog};

use super::{Drive, Placement, Transform};
use crate::slots::locators_by_slot;
use specsheet_data::model::ModelCache;
use specsheet_data::GameFs;

/// Base models (chassis, cabin) of a configuration: their placements,
/// visible locators.
pub(super) struct BaseLayout {
    pub(super) placements: Vec<Placement>,
    pub(super) locators: Vec<Locator>,
    pub(super) warnings: Vec<String>,
}

pub(super) fn base_layout(
    fs: &GameFs,
    cache: &ModelCache,
    catalog: &TruckCatalog,
    config: &Configuration,
    drive: Drive,
) -> BaseLayout {
    let mut layout = BaseLayout {
        placements: Vec::new(),
        locators: Vec::new(),
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
                layout
                    .placements
                    .extend(companions(&accessory.model, drive).map(|model| Placement {
                        slot: accessory.slot.clone(),
                        accessory: accessory.id.clone(),
                        model: model.to_owned(),
                        look: None,
                        variant: None,
                        transform: Transform::IDENTITY,
                    }));
            }
            Err(error) => layout.warnings.push(format!("{}: {error}", accessory.id)),
        }
    }
    layout
}

/// The models a base part brings in its own space: a cabin's side
/// windows' glass and the wipers for `drive`, a chassis's shadow quad.
fn companions(model: &ModelRef, drive: Drive) -> impl Iterator<Item = &str> {
    let wipers = match drive {
        Drive::Right => model.wipers_uk.as_ref().or(model.wipers.as_ref()),
        Drive::Left => model.wipers.as_ref(),
    };
    model
        .windows
        .iter()
        .chain(wipers)
        .chain(&model.ui_shadow)
        .map(String::as_str)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cabin_brings_its_side_windows_and_the_wipers_for_its_drive() {
        // `/def/vehicle/truck/scania.s_2016/cabin/highline.sii`.
        let cabin = ModelRef {
            exterior: Some("/cab_s_high.pmd".to_owned()),
            windows: Some("/win_open_s.pmd".to_owned()),
            wipers: Some("/wiper_s_eu.pmd".to_owned()),
            wipers_uk: Some("/wiper_s_uk.pmd".to_owned()),
            ..ModelRef::default()
        };
        let left: Vec<&str> = companions(&cabin, Drive::Left).collect();
        assert_eq!(left, ["/win_open_s.pmd", "/wiper_s_eu.pmd"]);
        let right: Vec<&str> = companions(&cabin, Drive::Right).collect();
        assert_eq!(right, ["/win_open_s.pmd", "/wiper_s_uk.pmd"]);
        let no_uk = ModelRef {
            wipers: Some("/wiper.pmd".to_owned()),
            ..ModelRef::default()
        };
        assert_eq!(
            companions(&no_uk, Drive::Right).collect::<Vec<_>>(),
            ["/wiper.pmd"]
        );
        assert_eq!(companions(&ModelRef::default(), Drive::Left).count(), 0);
        // `/def/vehicle/truck/volvo.fh_2024/chassis/4x2.sii`.
        let chassis = ModelRef {
            ui_shadow: Some("/ui_shadow_4x2.pmd".to_owned()),
            ..ModelRef::default()
        };
        assert_eq!(
            companions(&chassis, Drive::Left).collect::<Vec<_>>(),
            ["/ui_shadow_4x2.pmd"]
        );
    }
}
