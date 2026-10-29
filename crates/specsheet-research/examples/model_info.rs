//! Research tool: prints the looks, variants, parts and locators of a model.
//! Usage: ETS2_DIR=... cargo run -p specsheet-research --example model_info -- /vehicle/.../x.pmd

use specsheet_data::{GameFs, GameInstall};
use specsheet_formats::{pmd::Pmd, pmg::Pmg};

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
                    .and_then(|b| specsheet_formats::mat::Material::parse(&b).ok())
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
            for (index, piece) in pmg.pieces[part.pieces.clone()].iter().enumerate() {
                let (lo, hi) =
                    piece
                        .positions
                        .iter()
                        .fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), p| {
                            (
                                std::array::from_fn(|i| lo[i].min(p[i])),
                                std::array::from_fn(|i| hi[i].max(p[i])),
                            )
                        });
                println!(
                    "    piece {:<3} material {:<3} vertices {:<6} bounds {lo:.2?}..{hi:.2?}",
                    part.pieces.start + index,
                    piece.material,
                    piece.positions.len()
                );
            }
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
