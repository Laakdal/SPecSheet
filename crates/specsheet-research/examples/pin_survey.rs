//! Research tool for finding F5: builds every truck preset's exterior pins
//! the game's way and measures how far each pin lands from the bounding box
//! of the parts placed in its slot. Pins far from their parts point at a
//! wrong base locator or model order.
//! Usage: ETS2_DIR=... cargo run --release -p specsheet-research --example pin_survey

use std::collections::BTreeMap;

use specsheet_assembly::Drive;
use specsheet_data::GameInstall;
use specsheet_session::view::{self, HotspotCategory};
use specsheet_session::GameSession;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let game = GameSession::open(GameInstall::discover()?)?;
    println!("pin table: {} entries", game.pin_table().len());
    let mut distances: Vec<(f32, String)> = Vec::new();
    let mut per_slot: BTreeMap<String, (usize, f32)> = BTreeMap::new();
    let mut unplaced = 0usize;
    for truck in view::trucks(&game) {
        let mut session = game.start_truck(&truck.id)?;
        session.set_drive(&game, Drive::Left);
        let overview = view::overview(&game, &session);
        let assembly = session.assembly();
        for hotspot in overview
            .hotspots
            .iter()
            .filter(|h| h.category != HotspotCategory::Interior)
        {
            let mut min = [f32::MAX; 3];
            let mut max = [f32::MIN; 3];
            for placement in assembly
                .placements
                .iter()
                .filter(|p| p.slot == hotspot.slot)
            {
                let Ok(model) = game.models().load(game.fs(), &placement.model) else {
                    continue;
                };
                let t = &placement.transform;
                let [lo, hi] = [model.pmg.aabb.min, model.pmg.aabb.max];
                for corner in 0..8 {
                    let local = [
                        if corner & 1 == 0 { lo[0] } else { hi[0] },
                        if corner & 2 == 0 { lo[1] } else { hi[1] },
                        if corner & 4 == 0 { lo[2] } else { hi[2] },
                    ];
                    let p = rotate(t.rotation, local.map(|c| c * t.scale));
                    for axis in 0..3 {
                        let v = p[axis] + t.translation[axis];
                        min[axis] = min[axis].min(v);
                        max[axis] = max[axis].max(v);
                    }
                }
            }
            if min[0] > max[0] {
                unplaced += 1;
                continue;
            }
            let d = (0..3)
                .map(|a| {
                    let p = hotspot.position[a];
                    (min[a] - p).max(0.0).max(p - max[a])
                })
                .map(|e| e * e)
                .sum::<f32>()
                .sqrt();
            distances.push((d, format!("{} {}", truck.id, hotspot.slot)));
            let entry = per_slot.entry(hotspot.slot.to_string()).or_default();
            entry.0 += 1;
            entry.1 = entry.1.max(d);
        }
    }
    distances.sort_by(|a, b| a.0.total_cmp(&b.0));
    let n = distances.len();
    let at = |q: f32| distances[((n as f32 - 1.0) * q) as usize].0;
    println!(
        "{n} exterior pins on placed parts ({unplaced} on empty slots): median {:.3} m, p90 {:.3} m, max {:.3} m",
        at(0.5),
        at(0.9),
        at(1.0)
    );
    println!("--- worst 25");
    for (d, what) in distances.iter().rev().take(25) {
        println!("{d:7.3}  {what}");
    }
    println!("--- per slot: pins, worst distance");
    for (slot, (count, worst)) in per_slot {
        println!("{slot:14} {count:5} {worst:7.3}");
    }
    Ok(())
}

/// Rotates `v` by the unit quaternion `(x, y, z, w)`.
fn rotate([x, y, z, w]: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let u = [x, y, z];
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let t = cross(u, v).map(|c| 2.0 * c);
    let c = cross(u, t);
    [
        v[0] + w * t[0] + c[0],
        v[1] + w * t[1] + c[1],
        v[2] + w * t[2] + c[2],
    ]
}
