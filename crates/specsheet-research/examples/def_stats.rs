//! Research tool: tallies unit classes and attribute keys of every truck
//! definition under a root directory (default /def/vehicle/truck).
//! Usage: def_stats [root dir]

use std::collections::BTreeMap;

use specsheet_data::{GameFs, GameInstall};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (vfs, _) = GameInstall::discover()?.mount()?;
    let fs = GameFs::new(vfs);
    let mut classes: BTreeMap<String, (usize, BTreeMap<String, usize>)> = BTreeMap::new();
    let root = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/def/vehicle/truck".to_owned());
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        let listing = fs.list_dir(&dir)?;
        stack.extend(listing.directories.iter().map(|d| format!("{dir}/{d}")));
        for f in listing.files.iter().filter(|f| f.ends_with(".sii")) {
            let doc = fs.read_sii(&format!("{dir}/{f}"))?;
            for unit in doc.units {
                let entry = classes.entry(unit.class.clone()).or_default();
                entry.0 += 1;
                let mut seen = std::collections::BTreeSet::new();
                for a in unit.attributes {
                    if seen.insert(a.key.clone()) {
                        *entry.1.entry(a.key).or_default() += 1;
                    }
                }
            }
        }
    }
    for (class, (n, keys)) in classes {
        println!("{class}: {n}");
        for (k, c) in keys {
            println!("    {k}: {c}");
        }
    }
    Ok(())
}
