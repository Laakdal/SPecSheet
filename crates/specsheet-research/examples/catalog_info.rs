//! Research tool: loads every truck catalog and preset, prints a summary.

use specsheet_data::{list_trucks, load_catalog, load_preset, GameFs, GameInstall};
use specsheet_truck::{AccessoryKind, SlotAvailability};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (vfs, _) = GameInstall::discover()?.mount()?;
    let fs = GameFs::new(vfs);
    for truck in list_trucks(&fs)? {
        let start = std::time::Instant::now();
        let (catalog, warnings) = load_catalog(&fs, &truck.id)?;
        let count = |k| catalog.accessories().filter(|a| a.kind == k).count();
        let all_slots = SlotAvailability {
            available: catalog.slots().into_iter().collect(),
        };
        let preset = load_preset(&fs, &catalog)?.map(|(p, w)| {
            for w in &w {
                println!("    WARN {}: {}", w.path, w.message);
            }
            p
        });
        let issues = preset.as_ref().map(|p| p.validate(&catalog, &all_slots));
        println!(
            "{:<22} {:>5} acc ({} chassis, {} cabins, {} addons, {} paint) {:>3} slots, {} warnings, preset: {} sel, issues {:?} [{:?}]",
            truck.id, catalog.accessories().count(), count(AccessoryKind::Chassis), count(AccessoryKind::Cabin),
            count(AccessoryKind::Addon), count(AccessoryKind::PaintJob), catalog.slots().len(), warnings.len(),
            preset.as_ref().map_or(0, |p| p.selections().len()), issues.map(|i| i.len()), start.elapsed()
        );
        if let Some(issues) = preset.as_ref().map(|p| p.validate(&catalog, &all_slots)) {
            for i in issues.iter().take(3) {
                println!("      {i:?}");
            }
        }
    }
    Ok(())
}
