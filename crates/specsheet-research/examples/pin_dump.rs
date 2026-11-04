//! Research tool: the exterior pins of trucks' starting configurations, as
//! the app places them (slot, truck-space position, facing), sorted front to
//! back, and the pairs that land within 5 cm of each other. With a truck id,
//! that truck in full, after selecting any accessory ids that follow it;
//! without one, every truck's overlaps and a summary.
//!
//! `cargo run --release -p specsheet-research --example pin_dump [-- <truck id> [<accessory id>…]]`

use specsheet_core::{AccessoryId, TruckId};
use specsheet_data::GameInstall;
use specsheet_session::view::{self, Hotspot, Tab};
use specsheet_session::GameSession;

/// Pins closer than this overlap on screen.
const OVERLAP: f32 = 0.05;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let game = GameSession::open(GameInstall::discover()?)?;
    let one = std::env::args().nth(1);
    let selections: Vec<AccessoryId> = std::env::args()
        .skip(2)
        .map(|a| AccessoryId::from(a.as_str()))
        .collect();
    let trucks: Vec<TruckId> = match &one {
        Some(id) => vec![TruckId::from(id.as_str())],
        None => view::trucks(&game).into_iter().map(|t| t.id).collect(),
    };
    let (mut total_pins, mut total_overlaps) = (0, 0);
    for truck in &trucks {
        let mut session = match game.start_truck(truck) {
            Ok(session) => session,
            Err(error) => {
                println!("{truck}: {error}");
                continue;
            }
        };
        for accessory in &selections {
            if let Err(error) = session.select(&game, accessory) {
                println!("{accessory}: {error}");
            }
        }
        let overview = view::overview(&game, &session);
        let mut pins: Vec<&Hotspot> = overview
            .hotspots
            .iter()
            .filter(|h| h.tab == Tab::ExteriorAccessories && h.link.is_none())
            .collect();
        pins.sort_by(|a, b| a.position[2].total_cmp(&b.position[2]));
        if one.is_some() {
            println!("slot: position (x right, y up, z back); facing; selected; part at");
            for pin in &pins {
                let [x, y, z] = pin.position;
                let facing = pin
                    .facing
                    .map(|[fx, fy, fz]| format!("({fx:.2}, {fy:.2}, {fz:.2})"))
                    .unwrap_or_default();
                let part = session
                    .assembly()
                    .placements
                    .iter()
                    .find(|p| p.slot == pin.slot)
                    .map(|p| {
                        let [px, py, pz] = p.transform.translation;
                        format!("({px:.3}, {py:.3}, {pz:.3})")
                    })
                    .unwrap_or_else(|| "-".into());
                println!(
                    "  {}: ({x:.3}, {y:.3}, {z:.3}); {facing}; {}; {part}",
                    pin.slot,
                    pin.selected.as_deref().unwrap_or("-")
                );
            }
        }
        let mut overlaps = Vec::new();
        for (i, a) in pins.iter().enumerate() {
            for b in &pins[i + 1..] {
                let d = distance(a.position, b.position);
                if d < OVERLAP {
                    overlaps.push(format!("{} and {}: {d:.3} m", a.slot, b.slot));
                }
            }
        }
        total_pins += pins.len();
        total_overlaps += overlaps.len();
        println!(
            "{truck}: {} exterior pins, {} overlapping pairs",
            pins.len(),
            overlaps.len()
        );
        for overlap in overlaps {
            println!("  {overlap}");
        }
    }
    println!(
        "{} trucks, {total_pins} exterior pins, {total_overlaps} pairs within {OVERLAP} m",
        trucks.len()
    );
    Ok(())
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.iter()
        .zip(b)
        .map(|(p, q)| (p - q) * (p - q))
        .sum::<f32>()
        .sqrt()
}
