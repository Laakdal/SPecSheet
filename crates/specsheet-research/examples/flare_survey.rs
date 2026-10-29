//! Research tool: which vehicle light hookups (`flare.*`) the truck
//! accessories' exterior models name on their locators, whether each
//! resolves to a unit in `/unit/hookup/vehicle/flare/`, and whether its glow
//! model exists. Output feeds `research/evidence/flare-survey.txt`.

use std::collections::{BTreeMap, BTreeSet};

use specsheet_data::flare::vehicle_lights;
use specsheet_data::model::ModelCache;
use specsheet_data::{list_trucks, load_catalog, GameFs, GameInstall};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (vfs, _) = GameInstall::discover()?.mount()?;
    let fs = GameFs::new(vfs);
    let (lights, warnings) = vehicle_lights(&fs)?;
    for warning in &warnings {
        println!("warning: {warning}");
    }
    let cache = ModelCache::new();
    // hookup -> (locators, models)
    let mut uses: BTreeMap<String, (usize, BTreeSet<String>)> = BTreeMap::new();
    let mut other_hookups: BTreeMap<String, usize> = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut models = 0;
    for truck in list_trucks(&fs)? {
        let (catalog, _) = load_catalog(&fs, &truck.id)?;
        for accessory in catalog.accessories() {
            let Some(path) = accessory.model.exterior.as_ref() else {
                continue;
            };
            if !seen.insert(path.clone()) {
                continue;
            }
            let Ok(model) = cache.load(&fs, path) else {
                continue;
            };
            models += 1;
            for hookup in model.pmg.locators.iter().filter_map(|l| l.hookup.as_ref()) {
                if hookup.starts_with("flare.") {
                    let entry = uses.entry(hookup.clone()).or_default();
                    entry.0 += 1;
                    entry.1.insert(path.clone());
                } else {
                    *other_hookups.entry(hookup.clone()).or_default() += 1;
                }
            }
        }
    }
    println!(
        "{} light units in /unit/hookup/vehicle/flare; {models} distinct exterior models scanned",
        lights.len()
    );
    println!("hookup: locators, models; class, light_type, glow model (exists)");
    let (mut resolved, mut missing_models) = (0, 0);
    for (hookup, (locators, in_models)) in &uses {
        let line = match lights.get(hookup) {
            Some(light) => {
                resolved += 1;
                let model = light.model.as_deref().map_or("-".to_owned(), |m| {
                    let exists = fs.exists(m);
                    if !exists {
                        missing_models += 1;
                    }
                    format!("{m} ({})", if exists { "yes" } else { "NO" })
                });
                format!(
                    "{:?}, {}, {model}",
                    light.kind,
                    light.light_type.as_deref().unwrap_or("-")
                )
            }
            None => "UNRESOLVED".to_owned(),
        };
        println!("  {hookup}: {locators}, {}; {line}", in_models.len());
    }
    println!(
        "{} distinct flare hookups, {resolved} resolved, {missing_models} glow models missing",
        uses.len()
    );
    println!("other locator hookups:");
    for (hookup, count) in &other_hookups {
        println!("  {hookup}: {count}");
    }
    Ok(())
}
