//! Research tool: measures how wheel parts line up.
//!
//! Part 1 prints, for every shared wheel-part definition, the `tire_shift` /
//! `wheel_shift` attributes and the x extent of its `left` and `right`
//! variants (union of the visible pieces' bounding boxes, model space).
//! Part 2 prints, for every truck preset, the selected wheel parts per axle.
//! Usage: wheel_fit [group filter, e.g. f_]
//!        wheel_fit profile <model.pmd> <variant>  (x range per 1 cm radius band)
//!        wheel_fit fit  (for every rim/disc with `tire_shift`: where its outer
//!        lip sits relative to the stock tyre's outer bead, per side)
//!        wheel_fit locators  (chassis wheel locators with a rotation)

use specsheet_core::Aabb;
use specsheet_data::model::ModelCache;
use specsheet_data::{list_trucks, load_catalog, load_preset, GameFs, GameInstall};
use specsheet_truck::AccessoryKind;

const GROUPS: &[&str] = &[
    "f_tire", "f_disc", "f_rim", "f_hub", "f_nuts", "f_cover", "f_wheel", "r_tire", "r_disc",
    "r_rim", "r_hub", "r_nuts", "r_cover", "r_wheel",
];

fn variant_box(model: &specsheet_data::model::Model, variant: &str) -> Option<Aabb> {
    let index = model.pmd.variant_index(variant)?;
    model
        .pmg
        .parts
        .iter()
        .enumerate()
        .filter(|(i, _)| model.pmd.is_part_visible(index, *i))
        .flat_map(|(_, part)| &model.pmg.pieces[part.pieces.clone()])
        .map(|piece| piece.aabb)
        .reduce(|a, b| a.union(&b))
}

fn fmt_x(aabb: Option<Aabb>) -> String {
    match aabb {
        Some(b) => format!(
            "x[{:+.4} {:+.4}] cx {:+.4} r {:.4}",
            b.min[0],
            b.max[0],
            (b.min[0] + b.max[0]) / 2.0,
            b.max[1]
        ),
        None => "-".to_owned(),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let filter = args.first().cloned().unwrap_or_default();
    let (vfs, _) = GameInstall::discover()?.mount()?;
    let fs = GameFs::new(vfs);
    let cache = ModelCache::new();
    if filter == "locators" {
        return rotated_wheel_locators(&fs, &cache);
    }
    if filter == "fit" {
        return fit(&fs, &cache);
    }
    if filter == "profile" {
        let (path, variant) = (&args[1], &args[2]);
        let model = cache.load(&fs, path)?;
        let index = model.pmd.variant_index(variant).ok_or("no such variant")?;
        let mut bands: std::collections::BTreeMap<i32, (f32, f32, usize)> = Default::default();
        for (_, part) in model
            .pmg
            .parts
            .iter()
            .enumerate()
            .filter(|(i, _)| model.pmd.is_part_visible(index, *i))
        {
            for piece in &model.pmg.pieces[part.pieces.clone()] {
                for p in &piece.positions {
                    let r = (p[1] * p[1] + p[2] * p[2]).sqrt();
                    let band =
                        bands
                            .entry((r * 100.0).floor() as i32)
                            .or_insert((f32::MAX, f32::MIN, 0));
                    band.0 = band.0.min(p[0]);
                    band.1 = band.1.max(p[0]);
                    band.2 += 1;
                }
            }
        }
        println!("# {path} {variant}: radius band (cm) | x min | x max | vertices");
        for (band, (lo, hi, n)) in bands {
            println!("{band:3} | {lo:+.4} | {hi:+.4} | {n}");
        }
        return Ok(());
    }
    println!("# part definitions: group file unit | tire_shift wheel_shift | model | left | right");
    for group in GROUPS.iter().filter(|g| g.starts_with(&filter)) {
        let dir = format!("/def/vehicle/{group}");
        for file in fs.list_dir(&dir)?.files {
            let path = format!("{dir}/{file}");
            for unit in fs.read_sii(&path)?.units {
                let Some(model_path) = unit.get_str("model") else {
                    continue;
                };
                let (left, right) = match cache.load(&fs, model_path) {
                    Ok(m) => (variant_box(&m, "left"), variant_box(&m, "right")),
                    Err(_) => (None, None),
                };
                println!(
                    "{group} {file} {} | ts {:?} ws {:?} | {model_path} | L {} | R {}",
                    unit.name,
                    unit.get_f32("tire_shift"),
                    unit.get_f32("wheel_shift"),
                    fmt_x(left),
                    fmt_x(right),
                );
            }
        }
    }
    println!("# presets: truck | wheel selections");
    for truck in list_trucks(&fs)? {
        let (catalog, _) = load_catalog(&fs, &truck.id)?;
        let Some((preset, _)) = load_preset(&fs, &catalog)? else {
            continue;
        };
        let wheels: Vec<String> = preset
            .selections()
            .values()
            .filter_map(|id| catalog.accessory(id))
            .filter(|a| a.kind == AccessoryKind::Wheel)
            .map(|a| a.def_path.trim_start_matches("/def/vehicle/").to_owned())
            .collect();
        println!("{} | {}", truck.id, wheels.join(" "));
    }
    Ok(())
}

/// Outermost (most outboard) x of the visible vertices of `variant` whose
/// radius about the x axis lies in `radius`. Outboard is -x for `left`.
fn outboard_x(
    model: &specsheet_data::model::Model,
    variant: &str,
    radius: std::ops::Range<f32>,
) -> Option<f32> {
    let index = model.pmd.variant_index(variant)?;
    let sign = if variant == "left" { -1.0 } else { 1.0 };
    model
        .pmg
        .parts
        .iter()
        .enumerate()
        .filter(|(i, _)| model.pmd.is_part_visible(index, *i))
        .flat_map(|(_, part)| &model.pmg.pieces[part.pieces.clone()])
        .flat_map(|piece| &piece.positions)
        .filter(|p| radius.contains(&(p[1] * p[1] + p[2] * p[2]).sqrt()))
        .map(|p| p[0] * sign)
        .reduce(f32::max)
        .map(|x| x * sign)
}

fn fit(fs: &GameFs, cache: &ModelCache) -> Result<(), Box<dyn std::error::Error>> {
    println!("# group file | tire_shift (absent = 0) | side | tyre bead x | lip x | lip-bead | residual = (lip-bead) - side*ts");
    println!("# side = +1 for left, -1 for right. bead: outboard-most tyre vertex within 2 cm of the tyre's min radius;");
    println!("# lip: outboard-most rim vertex within 2 cm of the rim's max radius.");
    let mut within = std::collections::BTreeMap::<&str, (usize, usize)>::new();
    for (group, tyre) in [
        ("f_disc", "/vehicle/truck/upgrade/wheel/tire/front_1.pmd"),
        ("r_disc", "/vehicle/truck/upgrade/wheel/tire/rear_1.pmd"),
        ("f_rim", "/vehicle/truck/upgrade/wheel/tire/front_1.pmd"),
        ("r_rim", "/vehicle/truck/upgrade/wheel/tire/rear_1.pmd"),
    ] {
        let tyre = cache.load(fs, tyre)?;
        let min_r = tyre_min_radius(&tyre);
        let dir = format!("/def/vehicle/{group}");
        for file in fs.list_dir(&dir)?.files {
            for unit in fs.read_sii(&format!("{dir}/{file}"))?.units {
                let Some(model) = unit.get_str("model") else {
                    continue;
                };
                let ts = unit.get_f32("tire_shift").unwrap_or(0.0);
                let Ok(rim) = cache.load(fs, model) else {
                    continue;
                };
                let max_r = rim.pmg.aabb.max[1];
                for (variant, side) in [("left", 1.0f32), ("right", -1.0)] {
                    let bead = outboard_x(&tyre, variant, min_r..(min_r + 0.02));
                    let lip = outboard_x(&rim, variant, (max_r - 0.02)..f32::MAX);
                    if let (Some(bead), Some(lip)) = (bead, lip) {
                        let residual = lip - bead - side * ts;
                        let entry = within.entry(group).or_default();
                        entry.1 += 1;
                        if residual.abs() <= 0.005 {
                            entry.0 += 1;
                        }
                        println!(
                            "{group} {file} | {ts:+.4} | {variant:5} | {bead:+.4} | {lip:+.4} | {:+.4} | {residual:+.4}",
                            lip - bead,
                        );
                    }
                }
            }
        }
    }
    for (group, (ok, n)) in within {
        println!("# {group}: {ok} of {n} part x side cases have |residual| <= 5 mm");
    }
    Ok(())
}

fn tyre_min_radius(model: &specsheet_data::model::Model) -> f32 {
    model
        .pmg
        .pieces
        .iter()
        .flat_map(|piece| &piece.positions)
        .map(|p| (p[1] * p[1] + p[2] * p[2]).sqrt())
        .fold(f32::MAX, f32::min)
}

fn rotated_wheel_locators(
    fs: &GameFs,
    cache: &ModelCache,
) -> Result<(), Box<dyn std::error::Error>> {
    let (mut total, mut rotated) = (0, 0);
    let mut seen = std::collections::BTreeSet::new();
    for truck in list_trucks(fs)? {
        let (catalog, _) = load_catalog(fs, &truck.id)?;
        for chassis in catalog
            .accessories()
            .filter(|a| a.kind == AccessoryKind::Chassis)
        {
            let Some(path) = &chassis.model.exterior else {
                continue;
            };
            if !seen.insert(path.clone()) {
                continue;
            }
            let Ok(model) = cache.load(fs, path) else {
                continue;
            };
            for l in model
                .pmg
                .locators
                .iter()
                .filter(|l| l.name.starts_with("wheel_"))
            {
                total += 1;
                let [w, x, y, z] = l.rotation_wxyz;
                if (w.abs() - 1.0).abs() > 1e-4 {
                    rotated += 1;
                    println!(
                        "{path} {} pos {:?} rot_wxyz [{w:.4}, {x:.4}, {y:.4}, {z:.4}]",
                        l.name, l.position
                    );
                }
            }
        }
    }
    println!(
        "# {rotated} of {total} wheel locators ({} chassis models) have a non-identity rotation",
        seen.len()
    );
    Ok(())
}
