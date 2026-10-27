//! Research tool: prints up to N units of a class that define a given key.
//! Usage: def_grep <class> <key> [max] [root dir, default /def/vehicle/truck]

use game_data::{GameFs, GameInstall};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (class, key) = (&args[0], &args[1]);
    let max: usize = args.get(2).and_then(|m| m.parse().ok()).unwrap_or(3);
    let invert = key.starts_with('!');
    let key = key.trim_start_matches('!');
    let (vfs, _) = GameInstall::discover()?.mount()?;
    let fs = GameFs::new(vfs);
    let mut found = 0;
    let root = args
        .get(3)
        .cloned()
        .unwrap_or_else(|| "/def/vehicle/truck".to_owned());
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        let listing = fs.list_dir(&dir)?;
        stack.extend(listing.directories.iter().map(|d| format!("{dir}/{d}")));
        for f in listing.files.iter().filter(|f| f.ends_with(".sii")) {
            let path = format!("{dir}/{f}");
            for unit in fs.read_sii(&path)?.units {
                let has = unit.attributes.iter().any(|a| a.key == *key);
                if unit.class == *class && has != invert {
                    println!("--- {path}");
                    for a in &unit.attributes {
                        println!(
                            "  {}{} = {}",
                            a.key,
                            match a.slot {
                                scs_formats::sii::Slot::Scalar => String::new(),
                                scs_formats::sii::Slot::Append => "[]".into(),
                                scs_formats::sii::Slot::Index(i) => format!("[{i}]"),
                            },
                            a.value
                        );
                    }
                    found += 1;
                    if found >= max {
                        return Ok(());
                    }
                }
            }
        }
    }
    Ok(())
}
