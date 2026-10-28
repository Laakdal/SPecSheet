//! Research tool: how accessory `icon` attributes map to UI icon files.
//!
//! Walks every `.sii` under a root (default /def/vehicle), collects the
//! `icon` value of every unit that has one, and checks each against
//! `/material/ui/accessory/<icon>.mat` and `.tobj`. It then reads the
//! `.mat` of each resolved icon (texture source) and tallies the size and
//! DXGI format of the `.tobj` it names.
//!
//! Usage: ui_icon_survey [root dir] [examples to print] [icon ...]
//! With explicit icons (names, or absolute `.tobj` paths), it only reports the
//! texture size and format of each.

use std::collections::{BTreeMap, BTreeSet};

use specsheet_data::{GameFs, GameInstall};
use specsheet_formats::sii::Value;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = args
        .first()
        .cloned()
        .unwrap_or_else(|| "/def/vehicle".to_owned());
    let examples: usize = args.get(1).and_then(|n| n.parse().ok()).unwrap_or(6);
    let (vfs, _) = GameInstall::discover()?.mount()?;
    let fs = GameFs::new(vfs);

    // Explicit icons after the first two arguments: report their texture only.
    if args.len() > 2 {
        for icon in &args[2..] {
            // An absolute path (`/material/ui/truck_config/cabin.tobj`) is read as given.
            let tobj = match icon.strip_prefix('/') {
                Some(path) => path.to_owned(),
                None => format!("material/ui/accessory/{icon}.tobj"),
            };
            match fs.vfs().read_texture(&tobj) {
                Ok(t) => println!(
                    "{icon}: {}x{} dxgi {} mips {}",
                    t.image.width, t.image.height, t.image.dxgi_format, t.image.mip_count
                ),
                Err(e) => println!("{icon}: {e}"),
            }
        }
        return Ok(());
    }

    // icon value -> (classes using it, first defining file)
    let mut icons: BTreeMap<String, (BTreeSet<String>, String)> = BTreeMap::new();
    let mut units_with_icon: BTreeMap<String, usize> = BTreeMap::new();
    let mut parse_errors = 0usize;
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let listing = fs.list_dir(&dir)?;
        stack.extend(listing.directories.iter().map(|d| format!("{dir}/{d}")));
        for f in listing.files.iter().filter(|f| f.ends_with(".sii")) {
            let path = format!("{dir}/{f}");
            let Ok(doc) = fs.read_sii(&path) else {
                parse_errors += 1;
                continue;
            };
            for unit in doc.units {
                let Some(Value::String(icon) | Value::Token(icon)) = unit.get("icon") else {
                    continue;
                };
                *units_with_icon.entry(unit.class.clone()).or_default() += 1;
                let e = icons
                    .entry(icon.clone())
                    .or_insert_with(|| (BTreeSet::new(), path.clone()));
                e.0.insert(unit.class.clone());
            }
        }
    }
    println!(
        "root {root}: {} distinct icon values, parse errors {parse_errors}",
        icons.len()
    );
    println!("units with an icon attribute, by class:");
    for (c, n) in &units_with_icon {
        println!("  {c}: {n}");
    }

    let mut resolved = 0usize;
    let mut missing: Vec<&String> = Vec::new();
    let mut with_slash = 0usize;
    let mut with_ext = 0usize;
    let mut sizes: BTreeMap<String, usize> = BTreeMap::new();
    let mut tex_errors = 0usize;
    let mut printed = 0usize;
    let mut source_rule_ok = 0usize;
    let mut mismatches = 0usize;
    let mut mat_headers: BTreeMap<String, usize> = BTreeMap::new();
    for (icon, (classes, file)) in &icons {
        if icon.contains('/') {
            with_slash += 1;
        }
        if icon.contains('.') {
            with_ext += 1;
        }
        let mat = format!("/material/ui/accessory/{icon}.mat");
        let tobj = format!("/material/ui/accessory/{icon}.tobj");
        if !fs.exists(&mat) {
            missing.push(icon);
            continue;
        }
        resolved += 1;
        let text = String::from_utf8_lossy(&fs.read(&mat)?).into_owned();
        // Two syntaxes occur: `effect : "<rfx>" { texture : "texture" { source : "x.tobj" } }`
        // and the older `material : "ui" { texture : "x.tobj" texture_name : "texture" }`.
        let header = text.lines().next().unwrap_or_default().trim().to_owned();
        *mat_headers
            .entry(header.trim_end_matches('{').trim().to_owned())
            .or_default() += 1;
        let source = text
            .lines()
            .map(str::trim)
            .find_map(|l| {
                l.strip_prefix("source")
                    .or_else(|| l.strip_prefix("texture").filter(|r| r.contains(".tobj")))
            })
            .and_then(|r| r.split('"').nth(1))
            .map(str::to_owned);
        let source_abs = source.as_deref().map(|s| {
            if s.starts_with('/') {
                s.to_owned()
            } else {
                format!("/material/ui/accessory/{}", resolve_dir(icon, s))
            }
        });
        if source_abs.as_deref() == Some(tobj.as_str()) {
            source_rule_ok += 1;
        } else if mismatches < examples {
            mismatches += 1;
            println!("source differs: {mat} -> {source_abs:?}");
        }
        let tex_path = source_abs.unwrap_or(tobj.clone());
        match fs.vfs().read_texture(tex_path.trim_start_matches('/')) {
            Ok(t) => {
                let key = format!(
                    "{}x{} dxgi {} mips {}",
                    t.image.width, t.image.height, t.image.dxgi_format, t.image.mip_count
                );
                let n = sizes.entry(key.clone()).or_default();
                *n += 1;
                if *n <= 3 {
                    println!("size example: {key}: {icon}");
                }
            }
            Err(_) => tex_errors += 1,
        }
        if printed < examples {
            printed += 1;
            println!(
                "\n--- icon {icon:?} ({}) first defined in {file}",
                classes.iter().cloned().collect::<Vec<_>>().join(",")
            );
            println!("    {mat} exists; {tobj} exists: {}", fs.exists(&tobj));
            for l in text.lines() {
                println!("    | {l}");
            }
        }
    }
    println!(
        "\nresolved to /material/ui/accessory/<icon>.mat: {resolved} of {}",
        icons.len()
    );
    println!(
        ".mat texture source == /material/ui/accessory/<icon>.tobj: {source_rule_ok} of {resolved}"
    );
    println!("icon values containing '/': {with_slash}; containing '.': {with_ext}");
    println!("texture read errors: {tex_errors}");
    println!(".mat first lines:");
    for (k, n) in &mat_headers {
        println!("  {k}: {n}");
    }
    println!("texture size / format histogram:");
    for (k, n) in &sizes {
        println!("  {k}: {n}");
    }
    println!("unresolved icons ({}):", missing.len());
    for icon in missing.iter().take(40) {
        let (classes, file) = &icons[*icon];
        println!(
            "  {icon:?} ({}) in {file}",
            classes.iter().cloned().collect::<Vec<_>>().join(",")
        );
    }
    Ok(())
}

/// Resolves a `.mat`-relative texture path against the icon's directory.
fn resolve_dir(icon: &str, source: &str) -> String {
    match icon.rsplit_once('/') {
        Some((dir, _)) => format!("{dir}/{source}"),
        None => source.to_owned(),
    }
}
