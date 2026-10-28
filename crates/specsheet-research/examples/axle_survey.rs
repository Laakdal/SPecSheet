//! Research tool for finding F13: for every chassis of every truck, groups
//! the visible wheel locators into axles (by z, within 5 cm) and prints them
//! next to the definition's per-axle arrays (`powered_axle`,
//! `steerable_axle`, `liftable_axle`).
//! Usage: ETS2_DIR=... cargo run --release -p specsheet-research --example axle_survey

use specsheet_data::model::ModelCache;
use specsheet_data::{list_trucks, load_catalog, GameFs, GameInstall};
use specsheet_truck::AccessoryKind;

/// Wheel locators closer than this along z belong to one axle. Left and
/// right locators of an axle differ by up to 0.5 mm in the game's models.
const AXLE_TOLERANCE: f32 = 0.05;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (vfs, _) = GameInstall::discover()?.mount()?;
    let fs = GameFs::new(vfs);
    let cache = ModelCache::new();
    let (mut chassis_count, mut count_mismatches) = (0, 0);
    for truck in list_trucks(&fs)? {
        let (catalog, _) = load_catalog(&fs, &truck.id)?;
        for chassis in catalog
            .accessories()
            .filter(|a| a.kind == AccessoryKind::Chassis)
        {
            let Some(path) = chassis.model.exterior.as_deref() else {
                continue;
            };
            chassis_count += 1;
            let document = fs.read_sii(&chassis.def_path)?;
            let unit = document
                .units_of_class("accessory_chassis_data")
                .next()
                .ok_or("no accessory_chassis_data unit")?;
            let flags = |key: &str| -> String {
                unit.array(key)
                    .iter()
                    .map(|v| if v.to_string() == "true" { '1' } else { '0' })
                    .collect()
            };
            let powered = flags("powered_axle");
            let model = cache.load(&fs, path)?;
            let variant = model.variant_index(chassis.model.variant.as_deref());
            let mut wheels: Vec<(f32, String)> = model
                .visible_locators(variant)
                .filter(|l| l.name.starts_with("wheel_"))
                .map(|l| (l.position[2], l.name.clone()))
                .collect();
            wheels.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut axles: Vec<Vec<String>> = Vec::new();
            let mut last_z = f32::NEG_INFINITY;
            for (z, name) in wheels {
                match axles.last_mut() {
                    Some(axle) if z - last_z <= AXLE_TOLERANCE => axle.push(name),
                    _ => axles.push(vec![name]),
                }
                last_z = z;
            }
            let shape: Vec<String> = axles
                .iter()
                .map(|names| {
                    let mut kinds: Vec<&str> = names.iter().map(|n| &n[6..7]).collect();
                    kinds.dedup();
                    format!("{}x{}", kinds.join("/"), names.len())
                })
                .collect();
            let mismatch = axles.len() != powered.len();
            count_mismatches += usize::from(mismatch);
            println!(
                "{}{:<32} def_axles={} powered={:<4} steer={:<4} lift={:<4} wheel_axles=[{}] {path}",
                if mismatch { "!! " } else { "   " },
                chassis.id.as_str(),
                powered.len(),
                powered,
                flags("steerable_axle"),
                flags("liftable_axle"),
                shape.join(" "),
            );
        }
    }
    println!("--- {chassis_count} chassis, {count_mismatches} where wheel axles != declared axles");
    Ok(())
}
