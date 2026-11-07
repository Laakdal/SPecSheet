//! Research tool: mounts a profile's local mods the game's way (D20) and
//! reports what reads: each active mod's outcome, the trucks the mods add,
//! and for each added truck its catalog, its default assembly, its models,
//! materials and base textures. Everything is only read. Output names mod
//! packages, trucks and counts, never the profile's name or the home path.
//!
//! `--texture <path>` (repeatable) instead reports, for each texture path,
//! the package it is read from and what it loads as (format, size, bytes).
//!
//! Usage (ETS2_DIR set, or the install is discovered):
//!   cargo run --release -p specsheet-research --example mod_mount -- "<home>" <profile folder> [truck id ...]
//!   cargo run --release -p specsheet-research --example mod_mount -- "<home>" <profile folder> --texture <path> ...

use std::collections::{BTreeMap, BTreeSet};

use specsheet_core::TruckId;
use specsheet_data::mods::{Home, ModStatus};
use specsheet_data::render::{build_mesh, load_cube_texture, load_texture};
use specsheet_data::GameInstall;
use specsheet_session::GameSession;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut textures = Vec::new();
    while let Some(at) = args.iter().position(|a| a == "--texture") {
        args.remove(at);
        if at < args.len() {
            textures.push(args.remove(at));
        }
    }
    let [home, profile, wanted @ ..] = args.as_slice() else {
        eprintln!("usage: mod_mount <home> <profile folder> [truck id ...]");
        std::process::exit(2);
    };
    let home = Home::at(home)?;
    let base = GameSession::open(GameInstall::discover()?)?;
    let start = std::time::Instant::now();
    let game = GameSession::open_with_profile(GameInstall::discover()?, &home, profile)?;
    eprintln!("opened with mods in {:?}", start.elapsed());

    let mut outcomes: BTreeMap<&str, usize> = BTreeMap::new();
    for entry in game.mods() {
        let (kind, detail) = match &entry.status {
            ModStatus::Mounted { folders } => ("mounted", format!("{folders:?}")),
            ModStatus::WorkshopMissing => ("workshop item missing", String::new()),
            ModStatus::Missing => ("missing", String::new()),
            ModStatus::Incompatible { versions } => ("incompatible", versions.join(" ")),
            ModStatus::Unreadable { reason } => ("unreadable", reason.clone()),
        };
        *outcomes.entry(kind).or_default() += 1;
        // Workshop items by id: their package name is a number, not a
        // name the user gave.
        let label = match entry.active.workshop_id() {
            Some(id) => format!("workshop {id}"),
            None => entry.active.package.clone(),
        };
        println!("  {kind:<12} {label} {detail}");
    }
    println!("active mods: {} {outcomes:?}", game.mods().len());
    println!("mount failures: {}", game.mount_failures.len());
    if !textures.is_empty() {
        for path in &textures {
            let from = game
                .fs()
                .vfs()
                .locate(path)
                .map(|d| {
                    d.path()
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                })
                .unwrap_or_default()
                .unwrap_or_default();
            println!("{path}: from {from}");
            match load_texture(game.fs(), path) {
                Ok(t) => {
                    let bytes: usize = t.mips.iter().map(|m| m.data.len()).sum();
                    let base = t.mips.first().map(|m| (m.width, m.height));
                    println!(
                        "  2D {:?} srgb {} {base:?} mips {} {bytes} bytes",
                        t.format,
                        t.srgb,
                        t.mips.len()
                    );
                }
                Err(e) => println!("  2D: {e}"),
            }
            if let Ok(t) = load_cube_texture(game.fs(), path) {
                let bytes: usize = t.faces.iter().flatten().map(|m| m.data.len()).sum();
                let base = t
                    .faces
                    .first()
                    .and_then(|f| f.first())
                    .map(|m| (m.width, m.height));
                println!("  cube {:?} {base:?} {bytes} bytes", t.format);
            }
        }
        return Ok(());
    }

    let before: BTreeSet<&TruckId> = base.trucks().iter().map(|t| &t.id).collect();
    let added: Vec<TruckId> = game
        .trucks()
        .iter()
        .map(|t| t.id.clone())
        .filter(|id| !before.contains(id))
        .collect();
    println!(
        "trucks: {} without mods, {} with; added {:?}",
        base.trucks().len(),
        game.trucks().len(),
        added
    );
    let trucks: Vec<TruckId> = if wanted.is_empty() {
        added
    } else {
        wanted.iter().map(|w| TruckId::from(w.as_str())).collect()
    };

    for truck in trucks {
        println!("{truck}:");
        let session = match game.start_truck(&truck) {
            Ok(session) => session,
            Err(e) => {
                println!("  does not start: {e}");
                continue;
            }
        };
        let assembly = session.assembly();
        println!(
            "  load warnings {}, assembly warnings {}, placements {}, interior parts {}",
            session.load_warnings().len(),
            assembly.warnings.len(),
            assembly.placements.len(),
            assembly.interior_parts.len()
        );
        for w in session.load_warnings().iter().take(5) {
            println!("    load: {w:?}");
        }
        for w in &assembly.warnings {
            println!("    assembly: {w}");
        }
        // For each slot without a locator: the locators of every placed
        // model whose name starts with the slot's name.
        let missing: BTreeSet<&str> = assembly
            .warnings
            .iter()
            .filter_map(|w| w.split_once(": no locator for slot ").map(|(_, s)| s))
            .collect();
        let placed: Vec<_> = assembly
            .placements
            .iter()
            .chain(&assembly.interior_parts)
            .chain(&assembly.cockpit)
            .collect();
        for slot in &missing {
            let mut hits = Vec::new();
            for placement in &placed {
                let Ok(model) = game.models().load(game.fs(), &placement.model) else {
                    continue;
                };
                let variant = model.variant_index(placement.variant.as_deref());
                let visible: BTreeSet<&str> = model
                    .visible_locators(variant)
                    .map(|l| l.name.as_str())
                    .collect();
                for locator in &model.pmg.locators {
                    if locator.name.starts_with(slot) {
                        let shown = if visible.contains(locator.name.as_str()) {
                            "visible"
                        } else {
                            "hidden in this variant"
                        };
                        hits.push(format!("{} in {} ({shown})", locator.name, placement.model));
                    }
                }
            }
            hits.sort();
            hits.dedup();
            println!("    slot {slot}: {} matching locators", hits.len());
            for hit in hits.iter().take(8) {
                println!("      {hit}");
            }
        }
        let (mut models, mut model_errors, mut material_warnings) = (0, Vec::new(), 0);
        let mut textures: BTreeMap<String, usize> = BTreeMap::new();
        let mut texture_errors: Vec<String> = Vec::new();
        let mut sizes: Vec<(usize, String)> = Vec::new();
        let mut seen = BTreeSet::new();
        let placements = assembly
            .placements
            .iter()
            .chain(&assembly.interior_parts)
            .chain(&assembly.cockpit);
        for placement in placements {
            if !seen.insert(placement.model.clone()) {
                continue;
            }
            models += 1;
            let model = match game.models().load(game.fs(), &placement.model) {
                Ok(model) => model,
                Err(e) => {
                    model_errors.push(format!("{}: {e}", placement.model));
                    continue;
                }
            };
            let (mesh, warnings) = build_mesh(
                game.fs(),
                &model,
                placement.look.as_deref(),
                placement.variant.as_deref(),
                &[],
            );
            material_warnings += warnings.len();
            for material in &mesh.materials {
                let Some(texture) = &material.base_texture else {
                    continue;
                };
                if !seen.insert(texture.clone()) {
                    continue;
                }
                let kind = match load_texture(game.fs(), texture) {
                    Ok(data) => {
                        let bytes: usize = data.mips.iter().map(|m| m.data.len()).sum();
                        let from = game
                            .fs()
                            .vfs()
                            .locate(texture)
                            .and_then(|d| d.path().file_name())
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        let size = data.mips.first().map(|m| (m.width, m.height));
                        sizes.push((bytes, format!("{texture} {size:?} from {from}")));
                        "read".to_owned()
                    }
                    Err(e) => {
                        let text = e.to_string();
                        if texture_errors.len() < 5 {
                            texture_errors.push(format!("{texture}: {text}"));
                        }
                        if text.contains("not a packed texture") {
                            "loose (.tobj + .dds)".to_owned()
                        } else {
                            "error".to_owned()
                        }
                    }
                };
                *textures.entry(kind).or_default() += 1;
            }
        }
        println!(
            "  models {models}, failing {}; material warnings {material_warnings}; base textures {textures:?}",
            model_errors.len()
        );
        sizes.sort_by_key(|(bytes, _)| std::cmp::Reverse(*bytes));
        let total: usize = sizes.iter().map(|(b, _)| b).sum();
        println!(
            "  base texture bytes {:.1} MB; largest:",
            total as f64 / 1e6
        );
        for (bytes, what) in sizes.iter().take(4) {
            println!("    {:.1} MB {what}", *bytes as f64 / 1e6);
        }
        for e in model_errors.iter().take(5) {
            println!("    model: {e}");
        }
        for e in &texture_errors {
            println!("    texture: {e}");
        }
    }
    Ok(())
}
