//! Research tool: which vehicle light hookups (`flare.*`) the truck
//! accessories' exterior models name on their locators, whether each
//! resolves to a unit in `/unit/hookup/vehicle/flare/`, and whether its glow
//! model exists. Output feeds `research/evidence/flare-survey.txt`. With
//! `--csv <file>`, the hookup table is also written as CSV.
//! Usage: ETS2_DIR=... cargo run --release -p specsheet-research --example flare_survey [-- --csv <file>]

use std::collections::{BTreeMap, BTreeSet};

use specsheet_data::flare::vehicle_lights;
use specsheet_data::model::ModelCache;
use specsheet_data::{list_trucks, load_catalog, GameFs, GameInstall};
use specsheet_research::evidence::{csv_line, Header};

const USAGE: &str = "usage: flare_survey [--csv <file>]";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let csv = csv_path()?;
    let header = Header {
        title: "Flare survey: the light hookups of truck accessories and their units",
        exe_sha256: None,
        clean_room: false,
    };
    print!("{}", header.render());
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
    let mut table = csv_line(&[
        "hookup",
        "locators",
        "models",
        "class",
        "light_type",
        "glow_model",
        "glow_exists",
    ]);
    for (hookup, (locators, in_models)) in &uses {
        let mut row = vec![
            hookup.clone(),
            locators.to_string(),
            in_models.len().to_string(),
        ];
        let line = match lights.get(hookup) {
            Some(light) => {
                resolved += 1;
                let light_type = light.light_type.as_deref().unwrap_or("-");
                row.extend([format!("{:?}", light.kind), light_type.to_owned()]);
                let model = match light.model.as_deref() {
                    Some(m) => {
                        let exists = fs.exists(m);
                        if !exists {
                            missing_models += 1;
                        }
                        row.extend([m.to_owned(), if exists { "yes" } else { "no" }.to_owned()]);
                        format!("{m} ({})", if exists { "yes" } else { "NO" })
                    }
                    None => {
                        row.extend(["-".to_owned(), "-".to_owned()]);
                        "-".to_owned()
                    }
                };
                format!("{:?}, {light_type}, {model}", light.kind)
            }
            None => {
                row.extend(["UNRESOLVED", "-", "-", "-"].map(str::to_owned));
                "UNRESOLVED".to_owned()
            }
        };
        println!("  {hookup}: {locators}, {}; {line}", in_models.len());
        let fields: Vec<&str> = row.iter().map(String::as_str).collect();
        table.push_str(&csv_line(&fields));
    }
    if let Some(path) = &csv {
        std::fs::write(path, table)?;
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

/// The file named by `--csv <file>`, if any.
fn csv_path() -> Result<Option<std::path::PathBuf>, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [] => Ok(None),
        [flag, file] if flag == "--csv" => Ok(Some(file.into())),
        _ => Err(USAGE.to_owned()),
    }
}
