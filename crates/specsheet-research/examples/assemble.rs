//! Research tool: assembles a truck preset and builds every mesh and texture,
//! reporting counts, warnings and timings.
//! Usage: assemble <truck id> [uk]

use std::collections::BTreeSet;

use specsheet_assembly::{assemble, Drive};
use specsheet_data::model::ModelCache;
use specsheet_data::render::{build_mesh, load_texture};
use specsheet_data::{load_catalog, load_preset, GameFs, GameInstall};
use specsheet_truck::TruckId;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let truck = TruckId::new(
        args.first()
            .cloned()
            .unwrap_or_else(|| "scania.s_2016".into()),
    );
    let drive = if args.get(1).is_some_and(|a| a == "uk") {
        Drive::Right
    } else {
        Drive::Left
    };
    let t = std::time::Instant::now();
    let (vfs, _) = GameInstall::discover()?.mount()?;
    let fs = GameFs::new(vfs);
    let (catalog, _) = load_catalog(&fs, &truck)?;
    let (preset, _) = load_preset(&fs, &catalog)?.ok_or("no preset")?;
    println!("loaded in {:?}", t.elapsed());
    let cache = ModelCache::new();
    let t = std::time::Instant::now();
    let assembly = assemble(&fs, &cache, &catalog, &preset, drive);
    println!(
        "assembled {} placements, {} anchors, {} available slots in {:?}",
        assembly.placements.len(),
        assembly.anchors.len(),
        assembly.availability.available.len(),
        t.elapsed()
    );
    for w in &assembly.warnings {
        println!("  WARN {w}");
    }
    let t = std::time::Instant::now();
    let mut textures = BTreeSet::new();
    let (mut tris, mut pieces) = (0, 0);
    let mut models = BTreeSet::new();
    for p in &assembly.placements {
        if !models.insert((p.model.clone(), p.look.clone(), p.variant.clone())) {
            continue;
        }
        let model = cache.load(&fs, &p.model)?;
        let (mesh, warnings) =
            build_mesh(&fs, &model, p.look.as_deref(), p.variant.as_deref(), &[]);
        for w in warnings {
            println!("  MESH WARN {w}");
        }
        pieces += mesh.pieces.len();
        tris += mesh
            .pieces
            .iter()
            .map(|p| p.indices.len() / 3)
            .sum::<usize>();
        textures.extend(mesh.materials.iter().filter_map(|m| m.base_texture.clone()));
    }
    println!(
        "{} unique models, {pieces} pieces, {tris} triangles in {:?}",
        models.len(),
        t.elapsed()
    );
    let t = std::time::Instant::now();
    let mut bytes = 0;
    for path in &textures {
        match load_texture(&fs, path) {
            Ok(tex) => bytes += tex.mips.iter().map(|m| m.data.len()).sum::<usize>(),
            Err(e) => println!("  TEX ERR {path}: {e}"),
        }
    }
    println!(
        "{} textures, {} MiB in {:?}",
        textures.len(),
        bytes / (1 << 20),
        t.elapsed()
    );
    for a in assembly.anchors.iter().take(8) {
        println!("  anchor {} {:?}", a.slot, a.position);
    }
    Ok(())
}
