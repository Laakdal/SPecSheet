//! Research tool: prints the looks, variants, parts and locators of a model.
//! Usage: ETS2_DIR=... cargo run --example model_info -- /vehicle/.../x.pmd

use game_data::{GameFs, GameInstall};
use scs_formats::{pmd::Pmd, pmg::Pmg};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (vfs, _) = GameInstall::discover()?.mount()?;
    let fs = GameFs::new(vfs);
    for path in std::env::args().skip(1) {
        let pmd = Pmd::parse(&fs.read(&path)?)?;
        let pmg = Pmg::parse(&fs.read(&path.replace(".pmd", ".pmg"))?)?;
        println!("== {path}");
        println!(
            "looks: {:?}",
            pmd.looks.iter().map(|l| &l.name).collect::<Vec<_>>()
        );
        println!(
            "variants: {:?}",
            pmd.variants.iter().map(|v| &v.name).collect::<Vec<_>>()
        );
        println!("aabb: {:?}", pmg.aabb);
        if let Some(look) = pmd.looks.first() {
            for (i, m) in look.materials.iter().enumerate() {
                let effect = fs
                    .read(m)
                    .ok()
                    .and_then(|b| scs_formats::mat::Material::parse(&b).ok())
                    .map(|m| m.effect);
                println!("material {i}: {m} {effect:?}");
            }
        }
        for (i, part) in pmg.parts.iter().enumerate() {
            let visible: Vec<bool> = (0..pmd.variants.len())
                .map(|v| pmd.is_part_visible(v, i))
                .collect();
            println!(
                "part {:<16} pieces {:?} visible-by-variant {:?}",
                part.name, part.pieces, visible
            );
            for l in &pmg.locators[part.locators.clone()] {
                println!(
                    "    locator {:<14} pos {:?} rot {:?} scale {} hookup {:?}",
                    l.name, l.position, l.rotation_wxyz, l.scale, l.hookup
                );
            }
        }
    }
    Ok(())
}
