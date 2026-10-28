//! Research tool: what the materials of truck models say about shine and
//! reflection. For every distinct `.mat` used by a `/vehicle/truck` model it
//! tallies, per effect family (the effect name without its `eut2.` prefix and
//! `.rfx` suffix), each attribute's value range and each texture slot's
//! sources. It then prints the materials whose base texture is named like
//! chrome or metal.
//!
//! Usage: cargo run --release -p specsheet-research --example material_survey

use std::collections::{BTreeMap, BTreeSet};

use specsheet_data::{GameFs, GameInstall};
use specsheet_formats::mat::Material;
use specsheet_formats::pmd::Pmd;

/// Per effect family: material count, attribute ranges, texture sources.
type Family = (
    usize,
    BTreeMap<String, Range>,
    BTreeMap<String, BTreeSet<String>>,
);

#[derive(Default)]
struct Range {
    count: usize,
    min: Vec<f32>,
    max: Vec<f32>,
}

impl Range {
    fn add(&mut self, values: &[f32]) {
        if self.count == 0 {
            self.min = values.to_vec();
            self.max = values.to_vec();
        } else {
            for (i, v) in values.iter().enumerate() {
                if let (Some(lo), Some(hi)) = (self.min.get_mut(i), self.max.get_mut(i)) {
                    *lo = lo.min(*v);
                    *hi = hi.max(*v);
                }
            }
        }
        self.count += 1;
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (vfs, _) = GameInstall::discover()?.mount()?;
    let fs = GameFs::new(vfs);
    let mut materials = BTreeSet::new();
    let mut stack = vec!["/vehicle/truck".to_owned()];
    while let Some(dir) = stack.pop() {
        let listing = fs.list_dir(&dir)?;
        stack.extend(listing.directories.iter().map(|d| format!("{dir}/{d}")));
        for file in listing.files.iter().filter(|f| f.ends_with(".pmd")) {
            let Ok(pmd) = fs.read(&format!("{dir}/{file}")).map(|b| Pmd::parse(&b)) else {
                continue;
            };
            let Ok(pmd) = pmd else { continue };
            for look in &pmd.looks {
                materials.extend(look.materials.iter().cloned());
            }
        }
    }

    let mut families: BTreeMap<String, Family> = BTreeMap::new();
    let mut shiny = Vec::new();
    let mut errors = 0;
    for path in &materials {
        let Some(mat) = fs.read(path).ok().and_then(|b| Material::parse(&b).ok()) else {
            errors += 1;
            continue;
        };
        let family = mat
            .effect
            .trim_start_matches("eut2.")
            .trim_end_matches(".rfx")
            .to_owned();
        let entry = families.entry(family.clone()).or_default();
        entry.0 += 1;
        for (key, value) in &mat.attributes {
            if let Some(v) = value.as_floats() {
                entry.1.entry(key.clone()).or_default().add(&v);
            }
        }
        for texture in &mat.textures {
            let sources = entry.2.entry(texture.name.clone()).or_default();
            if sources.len() < 4 {
                if let Some(s) = &texture.source {
                    sources.insert(s.clone());
                }
            }
        }
        let base = mat
            .texture("texture_base")
            .and_then(|t| t.source.clone())
            .unwrap_or_default();
        if (base.contains("chrom") || base.contains("metal")) && shiny.len() < 25 {
            let attrs: Vec<String> = mat
                .attributes
                .iter()
                .map(|(k, v)| format!("{k}={v:?}"))
                .collect();
            let slots: Vec<String> = mat
                .textures
                .iter()
                .map(|t| format!("{}={}", t.name, t.source.as_deref().unwrap_or("-")))
                .collect();
            shiny.push(format!(
                "  {path}: {family}\n    {}\n    {}",
                attrs.join(" "),
                slots.join(" ")
            ));
        }
    }

    println!(
        "{} distinct truck materials, {errors} unreadable",
        materials.len()
    );
    let mut sorted: Vec<_> = families.into_iter().collect();
    sorted.sort_by_key(|f| std::cmp::Reverse(f.1 .0));
    for (family, (n, attrs, slots)) in sorted.iter().take(20) {
        println!("\n{family}: {n}");
        for (key, r) in attrs {
            println!("  {key:20} n={:5} min={:?} max={:?}", r.count, r.min, r.max);
        }
        for (slot, sources) in slots {
            println!("  texture {slot:20} e.g. {sources:?}");
        }
    }
    println!("\nMaterials with a chrome/metal base texture:");
    for line in shiny {
        println!("{line}");
    }
    Ok(())
}
