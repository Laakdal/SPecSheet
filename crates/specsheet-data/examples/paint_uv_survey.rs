//! Research tool for docs/app/shader-specs/truckpaint.md: the vertex streams
//! of truck paint pieces. For every `.pmd` under `vehicle/truck`, counts the
//! `truckpaint` pieces (by effect of their first look's material) per number
//! of UV sets and per presence of vertex colours, and averages the colours.
//! Usage: ETS2_DIR=... cargo run --release -p specsheet-data --example paint_uv_survey

use std::collections::BTreeMap;

use specsheet_data::GameInstall;
use specsheet_formats::{mat::Material, pmd::Pmd, pmg::Pmg};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (vfs, _) = GameInstall::discover()?.mount()?;
    let mut models = Vec::new();
    let mut stack = vec!["vehicle/truck".to_owned()];
    while let Some(dir) = stack.pop() {
        let Ok(listing) = vfs.list_dir(&dir) else {
            continue;
        };
        for sub in &listing.directories {
            stack.push(format!("{dir}/{sub}"));
        }
        for file in listing.files.iter().filter(|f| f.ends_with(".pmd")) {
            models.push(format!("{dir}/{file}"));
        }
    }
    models.sort();

    let mut effects: BTreeMap<String, String> = BTreeMap::new();
    // (effect, uv sets, has colours) -> (pieces, models)
    let mut rows: BTreeMap<(String, usize, bool), (usize, usize)> = BTreeMap::new();
    let mut examples: BTreeMap<(String, usize, bool), &str> = BTreeMap::new();
    let mut color_sum = [0f64; 4];
    let mut color_count = 0u64;
    let mut short: Vec<String> = Vec::new();
    let mut tinted = 0u64;
    // Paint pieces lacking the mask's UV set under two readings of "UV set
    // n": counted from 0 (mask 1, alt 2) and from the base set (mask 0, alt 1).
    let mut missing_from_0 = 0usize;
    let mut missing_from_base = 0usize;
    let mut paint_pieces = 0usize;
    for model in &models {
        let Ok(Ok(pmd)) = vfs.read(model).map(|b| Pmd::parse(&b)) else {
            continue;
        };
        let geometry = model.replace(".pmd", ".pmg");
        let Ok(Ok(pmg)) = vfs.read(&geometry).map(|b| Pmg::parse(&b)) else {
            continue;
        };
        let Some(look) = pmd.looks.first() else {
            continue;
        };
        let mut seen: BTreeMap<(String, usize, bool), usize> = BTreeMap::new();
        for piece in &pmg.pieces {
            let Some(material) = look.materials.get(piece.material as usize) else {
                continue;
            };
            let effect = effects
                .entry(material.clone())
                .or_insert_with(|| {
                    vfs.read(material.trim_start_matches('/'))
                        .ok()
                        .and_then(|b| Material::parse(&b).ok())
                        .map_or_else(|| "?".to_owned(), |m| m.effect)
                })
                .trim_end_matches(".rfx")
                .to_owned();
            if !effect.contains("truckpaint") {
                continue;
            }
            let altuv = effect.split('.').any(|t| t == "altuv");
            let sets = piece.uv_sets.len();
            paint_pieces += 1;
            missing_from_0 += usize::from(sets <= if altuv { 2 } else { 1 });
            missing_from_base += usize::from(sets <= usize::from(altuv));
            let key = (effect, sets, !piece.colors.is_empty());
            examples.entry(key.clone()).or_insert(model);
            *seen.entry(key.clone()).or_default() += 1;
            if piece.uv_sets.len() < 2 && short.len() < 20 {
                short.push(format!("{model}: {} uv sets", piece.uv_sets.len()));
            }
            for c in &piece.colors {
                let [r, g, b, _] = c.to_le_bytes();
                if r.abs_diff(g) > 2 || g.abs_diff(b) > 2 {
                    tinted += 1;
                }
                for (i, sum) in color_sum.iter_mut().enumerate() {
                    *sum += f64::from((c >> (8 * i)) & 0xff) / 255.0;
                }
                color_count += 1;
            }
        }
        for (key, pieces) in seen {
            let row = rows.entry(key).or_default();
            row.0 += pieces;
            row.1 += 1;
        }
    }
    println!("{} models", models.len());
    println!("pieces models uvsets colours effect");
    for ((effect, sets, colors), (pieces, users)) in &rows {
        let example = examples
            .get(&(effect.clone(), *sets, *colors))
            .copied()
            .unwrap_or("");
        println!("{pieces:6} {users:6} {sets:6} {colors:7} {effect}  e.g. {example}");
    }
    if color_count > 0 {
        let mean = color_sum.map(|s| s / color_count as f64);
        println!("mean vertex colour (bytes 0..3 / 255) over {color_count} vertices: {mean:.3?}");
    }
    println!("vertices with r, g, b more than 2 apart: {tinted}");
    println!(
        "{paint_pieces} paint pieces; lacking the mask's set if counted from 0 \
         (mask 1, altuv 2): {missing_from_0}; if mask 0, altuv 1: {missing_from_base}"
    );
    println!("first paint pieces with fewer than 2 UV sets:");
    for line in &short {
        println!("  {line}");
    }
    Ok(())
}
