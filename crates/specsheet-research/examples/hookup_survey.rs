//! Research tool: which accessory models carry `slot_NN` hookup locators,
//! per slot of the owning accessory, and how presets reference hookups.

use std::collections::{BTreeMap, BTreeSet};

use specsheet_data::model::ModelCache;
use specsheet_data::{list_trucks, load_catalog, GameFs, GameInstall};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (vfs, _) = GameInstall::discover()?.mount()?;
    let fs = GameFs::new(vfs);
    let cache = ModelCache::new();
    // host slot -> slot numbers seen -> count of models
    let mut by_host: BTreeMap<String, BTreeMap<u32, usize>> = BTreeMap::new();
    let mut models_with_slots: BTreeMap<String, usize> = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for truck in list_trucks(&fs)? {
        let (catalog, _) = load_catalog(&fs, &truck.id)?;
        for accessory in catalog.accessories() {
            let Some(path) = accessory.model.exterior.as_ref() else {
                continue;
            };
            if !seen.insert((accessory.slot.to_string(), path.clone())) {
                continue;
            }
            let Ok(model) = cache.load(&fs, path) else {
                continue;
            };
            let numbers: Vec<u32> = model
                .pmg
                .locators
                .iter()
                .filter_map(|l| l.name.strip_prefix("slot_").and_then(|n| n.parse().ok()))
                .collect();
            if numbers.is_empty() {
                continue;
            }
            *models_with_slots
                .entry(accessory.slot.to_string())
                .or_default() += 1;
            let entry = by_host.entry(accessory.slot.to_string()).or_default();
            for n in numbers {
                *entry.entry(n).or_default() += 1;
            }
        }
    }
    println!("host slot: models with slot_ locators; slot numbers (count)");
    for (host, numbers) in &by_host {
        let list: Vec<String> = numbers.iter().map(|(n, c)| format!("{n}({c})")).collect();
        println!(
            "{host}: {} models; {}",
            models_with_slots[host],
            list.join(" ")
        );
    }
    Ok(())
}
