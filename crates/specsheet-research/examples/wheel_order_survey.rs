//! Research tool for the wheel pin order (F5): for every chassis of every
//! truck, takes the visible `wheel_f*` / `wheel_r*` locators and checks three
//! candidate orders of each group against the pin builder's rule that even
//! wheels are left (x < 0) and both wheels of an axle are neighbours:
//! - `sort`: our order in `specsheet-assembly::pins::bases` (z, then x);
//! - `model`: the order the locators appear in the model;
//! - `name`: the wheel index in the locator name (`wheel_r_3_2` -> 3).
//!
//! Axles running front to back is reported separately from errors: the
//! rear group may be numbered from the back.
//!
//! Usage: ETS2_DIR=... cargo run --release -p specsheet-research --example wheel_order_survey

use std::collections::BTreeSet;

use specsheet_data::model::ModelCache;
use specsheet_data::{list_trucks, load_catalog, GameFs, GameInstall};
use specsheet_truck::AccessoryKind;

/// One wheel locator: name, x, z.
type Wheel = (String, f32, f32);

/// Wheels of one axle lie closer than this along z (`axle_survey`).
const AXLE_TOLERANCE: f32 = 0.05;

#[derive(Default)]
struct Tally {
    groups: usize,
    /// An even index holds a right wheel, or an odd one a left wheel.
    side_errors: usize,
    /// An axle's two wheels are not at indices 2k and 2k + 1.
    axle_errors: usize,
    /// Axles (pairs 2k, 2k + 1) run front to back / back to front.
    front_to_back: usize,
    back_to_front: usize,
    examples: Vec<String>,
}

impl Tally {
    fn check(&mut self, label: &str, wheels: &[Wheel]) {
        self.groups += 1;
        let sides_ok = wheels
            .iter()
            .enumerate()
            .all(|(i, w)| (i % 2 == 0) == (w.1 < 0.0));
        let axles_ok = wheels.chunks(2).all(|pair| match pair {
            [a, b] => (a.2 - b.2).abs() <= AXLE_TOLERANCE,
            _ => false,
        });
        let steps = || wheels.chunks(2).zip(wheels.chunks(2).skip(1));
        if wheels.len() > 2 {
            if steps().all(|(a, b)| a[0].2 < b[0].2) {
                self.front_to_back += 1;
            } else if steps().all(|(a, b)| a[0].2 > b[0].2) {
                self.back_to_front += 1;
            }
        }
        if !sides_ok {
            self.side_errors += 1;
        }
        if !axles_ok {
            self.axle_errors += 1;
        }
        if (!sides_ok || !axles_ok) && self.examples.len() < 12 {
            let list: Vec<String> = wheels
                .iter()
                .map(|(n, x, z)| format!("{n}({x:+.3},{z:+.4})"))
                .collect();
            self.examples.push(format!("{label}: {}", list.join(" ")));
        }
    }

    fn print(&self, order: &str) {
        println!(
            "{order:5}: {} groups, side errors {}, axle errors {}; with 2+ axles: front to back {}, back to front {}",
            self.groups, self.side_errors, self.axle_errors, self.front_to_back, self.back_to_front
        );
        for e in &self.examples {
            println!("    {e}");
        }
    }
}

/// The wheel index of a locator name: the first number after the group
/// (`wheel_r_3_2` -> 3, `wheel_f_1` -> 1, `wheel_f1` -> 1).
fn wheel_index(name: &str, group: &str) -> Option<u32> {
    let rest = name.strip_prefix(group)?.trim_start_matches('_');
    let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    rest.get(..digits)?.parse().ok()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (vfs, _) = GameInstall::discover()?.mount()?;
    let fs = GameFs::new(vfs);
    let cache = ModelCache::new();
    let mut seen = BTreeSet::new();
    let (mut sort, mut model, mut name) = (Tally::default(), Tally::default(), Tally::default());
    let mut model_is_name_order = 0usize;
    let mut unnumbered = 0usize;
    let mut max_pair_dz = 0.0f32;
    let mut prefixes = BTreeSet::new();
    for truck in list_trucks(&fs)? {
        let (catalog, _) = load_catalog(&fs, &truck.id)?;
        for chassis in catalog
            .accessories()
            .filter(|a| a.kind == AccessoryKind::Chassis)
        {
            let Some(path) = chassis.model.exterior.as_deref() else {
                continue;
            };
            if !seen.insert((path.to_owned(), chassis.model.variant.clone())) {
                continue;
            }
            let loaded = cache.load(&fs, path)?;
            let variant = loaded.variant_index(chassis.model.variant.as_deref());
            for group in ["wheel_f", "wheel_r"] {
                let in_model: Vec<Wheel> = loaded
                    .visible_locators(variant)
                    .filter(|l| l.name.starts_with(group))
                    .map(|l| (l.name.clone(), l.position[0], l.position[2]))
                    .collect();
                if in_model.is_empty() {
                    continue;
                }
                for w in &in_model {
                    prefixes.insert(
                        w.0.trim_end_matches(|c: char| c.is_ascii_digit())
                            .to_owned(),
                    );
                }
                let label = format!("{} {path} {group}", truck.id);
                let mut sorted = in_model.clone();
                sorted.sort_by(|a, b| a.2.total_cmp(&b.2).then(a.1.total_cmp(&b.1)));
                sort.check(&label, &sorted);
                model.check(&label, &in_model);
                if in_model.iter().all(|w| wheel_index(&w.0, group).is_some()) {
                    let mut by_name = in_model.clone();
                    by_name.sort_by_key(|w| wheel_index(&w.0, group));
                    if by_name
                        .iter()
                        .map(|w| &w.0)
                        .eq(in_model.iter().map(|w| &w.0))
                    {
                        model_is_name_order += 1;
                    }
                    name.check(&label, &by_name);
                } else {
                    unnumbered += 1;
                }
                for pair in sorted.chunks(2) {
                    if let [a, b] = pair {
                        if (a.2 - b.2).abs() <= AXLE_TOLERANCE {
                            max_pair_dz = max_pair_dz.max((a.2 - b.2).abs());
                        }
                    }
                }
            }
        }
    }
    println!("chassis models (path, variant): {}", seen.len());
    println!("wheel locator name stems: {prefixes:?}");
    println!(
        "groups whose model order equals name order: {model_is_name_order}; groups with an unnumbered name: {unnumbered}"
    );
    println!("largest z difference within an axle: {max_pair_dz:.5} m");
    sort.print("sort");
    model.print("model");
    name.print("name");
    Ok(())
}
