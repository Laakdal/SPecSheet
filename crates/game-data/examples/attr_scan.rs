//! Research tool: lists (class, key) pairs under a def directory whose key
//! contains a substring, with one example value each.
//! Usage: attr_scan <root dir> <substring>

use std::collections::BTreeMap;

use game_data::{GameFs, GameInstall};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (root, needle) = (args[0].clone(), args[1].clone());
    let (vfs, _) = GameInstall::discover()?.mount()?;
    let fs = GameFs::new(vfs);
    let mut found: BTreeMap<(String, String), (usize, String, String)> = BTreeMap::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        let Ok(listing) = fs.list_dir(&dir) else {
            continue;
        };
        stack.extend(listing.directories.iter().map(|d| format!("{dir}/{d}")));
        for f in listing.files.iter().filter(|f| f.ends_with(".sii")) {
            let path = format!("{dir}/{f}");
            let Ok(doc) = fs.read_sii(&path) else {
                continue;
            };
            for unit in doc.units {
                for a in unit.attributes.iter().filter(|a| a.key.contains(&needle)) {
                    let e = found.entry((unit.class.clone(), a.key.clone())).or_insert((
                        0,
                        a.value.to_string(),
                        path.clone(),
                    ));
                    e.0 += 1;
                }
            }
        }
    }
    for ((class, key), (n, example, path)) in found {
        println!("{class}.{key}: {n}  e.g. {example}  ({path})");
    }
    Ok(())
}
