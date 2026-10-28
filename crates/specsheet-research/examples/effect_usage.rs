//! Research tool for docs/app/shaders.md: which shader effects the truck and
//! UI scene models use. For every `.pmd` under `vehicle/truck`, `model/ui` and
//! `model2/ui`, reads the materials of every look and counts, per effect name
//! (from the `.mat` only), the models and distinct materials that use it.
//! Usage: ETS2_DIR=... cargo run --release -p specsheet-research --example effect_usage

use std::collections::{BTreeMap, BTreeSet};

use specsheet_data::GameInstall;
use specsheet_formats::{mat::Material, pmd::Pmd};

const ROOTS: &[&str] = &["vehicle/truck", "model/ui", "model2/ui"];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (vfs, _) = GameInstall::discover()?.mount()?;
    let mut models = Vec::new();
    let mut stack: Vec<String> = ROOTS.iter().map(|s| (*s).to_owned()).collect();
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

    let mut effect_of: BTreeMap<String, String> = BTreeMap::new();
    let mut models_of: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    let mut scene_uses: BTreeMap<String, usize> = BTreeMap::new();
    for model in &models {
        let Ok(pmd) = vfs.read(model).map(|b| Pmd::parse(&b)) else {
            continue;
        };
        let Ok(pmd) = pmd else { continue };
        let scene = model.starts_with("model");
        for material in pmd.looks.iter().flat_map(|l| &l.materials) {
            let effect = effect_of
                .entry(material.clone())
                .or_insert_with(|| {
                    vfs.read(material.trim_start_matches('/'))
                        .ok()
                        .and_then(|b| Material::parse(&b).ok())
                        .map_or_else(|| "?".to_owned(), |m| m.effect)
                })
                .trim_end_matches(".rfx")
                .to_owned();
            if scene {
                *scene_uses.entry(effect.clone()).or_default() += 1;
            }
            models_of.entry(effect).or_default().insert(model);
        }
    }

    let mut materials_of: BTreeMap<&str, usize> = BTreeMap::new();
    for effect in effect_of.values() {
        *materials_of
            .entry(effect.trim_end_matches(".rfx"))
            .or_default() += 1;
    }
    println!(
        "{} models, {} distinct materials, {} distinct effects",
        models.len(),
        effect_of.len(),
        models_of.len()
    );
    let mut rows: Vec<_> = models_of.iter().map(|(e, m)| (m.len(), e)).collect();
    rows.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
    println!("models  mats  scene  effect");
    for (count, effect) in &rows {
        println!(
            "{count:6} {:5} {:6}  {effect}",
            materials_of.get(effect.as_str()).unwrap_or(&0),
            scene_uses.get(*effect).unwrap_or(&0)
        );
    }
    let mut families: BTreeMap<&str, usize> = BTreeMap::new();
    for (effect, users) in &models_of {
        let family = effect.split('.').nth(1).unwrap_or("?");
        *families.entry(family).or_default() += users.len();
    }
    let mut families: Vec<_> = families.into_iter().collect();
    families.sort_by_key(|f| std::cmp::Reverse(f.1));
    println!("--- families (model uses): {families:?}");
    Ok(())
}
