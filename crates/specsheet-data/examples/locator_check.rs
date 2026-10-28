//! Research tool for finding F4: for every truck preset, matches addon slots
//! against the visible locators of the preset's chassis and cabin models.

use std::collections::BTreeSet;

use specsheet_data::model::ModelCache;
use specsheet_data::slots::locators_by_slot;
use specsheet_data::{list_trucks, load_catalog, load_preset, GameFs, GameInstall};
use specsheet_truck::{AccessoryKind, SlotId};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (vfs, _) = GameInstall::discover()?.mount()?;
    let fs = GameFs::new(vfs);
    let cache = ModelCache::new();
    let (mut total_selected, mut total_missing) = (0, 0);
    for truck in list_trucks(&fs)? {
        let (catalog, _) = load_catalog(&fs, &truck.id)?;
        let Some((preset, _)) = load_preset(&fs, &catalog)? else {
            continue;
        };
        let mut locators = Vec::new();
        let mut models = Vec::new();
        for id in preset.selections().values() {
            let Some(acc) = catalog.accessory(id) else {
                continue;
            };
            if !acc.kind.is_base() {
                continue;
            }
            let Some(path) = &acc.model.exterior else {
                continue;
            };
            models.push((cache.load(&fs, path)?, acc.model.variant.clone()));
        }
        for (model, variant) in &models {
            let v = model.variant_index(variant.as_deref());
            locators.extend(model.visible_locators(v));
        }
        let exterior_slots: BTreeSet<SlotId> = catalog
            .accessories()
            .filter(|a| a.kind == AccessoryKind::Addon && a.model.exterior.is_some())
            .map(|a| a.slot.clone())
            .collect();
        let by_slot = locators_by_slot(&exterior_slots, locators.iter().copied());
        let selected: Vec<&SlotId> = preset
            .selections()
            .iter()
            .filter(|(_, id)| {
                catalog
                    .accessory(id)
                    .is_some_and(|a| a.kind == AccessoryKind::Addon && a.model.exterior.is_some())
            })
            .map(|(s, _)| s)
            .collect();
        let missing: Vec<&str> = selected
            .iter()
            .filter(|s| !by_slot.contains_key(**s))
            .map(|s| s.as_str())
            .collect();
        let multi: Vec<String> = by_slot
            .iter()
            .filter(|(_, l)| l.len() > 1)
            .map(|(s, l)| format!("{s}x{}", l.len()))
            .collect();
        let unassigned: Vec<&str> = locators
            .iter()
            .filter(|l| {
                !exterior_slots
                    .iter()
                    .any(|s| specsheet_data::slots::matches_slot(s.as_str(), &l.name))
            })
            .map(|l| l.name.as_str())
            .collect();
        total_selected += selected.len();
        total_missing += missing.len();
        println!(
            "{:<20} selected-exterior {:>2} missing {:?}\n    multi {:?}\n    unassigned {:?}",
            truck.id,
            selected.len(),
            missing,
            multi,
            unassigned
        );
    }
    println!("TOTAL selected exterior addons {total_selected}, without locator {total_missing}");
    Ok(())
}
