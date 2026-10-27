//! Research tool: triangle winding vs stored normals, and material effects
//! used by truck models.

use std::collections::BTreeMap;

use specsheet_data::model::ModelCache;
use specsheet_data::{GameFs, GameInstall};
use specsheet_formats::mat::Material;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (vfs, _) = GameInstall::discover()?.mount()?;
    let fs = GameFs::new(vfs);
    let cache = ModelCache::new();
    let mut models = Vec::new();
    let mut stack = vec!["/vehicle/truck".to_owned()];
    while let Some(dir) = stack.pop() {
        let l = fs.list_dir(&dir)?;
        stack.extend(l.directories.iter().map(|d| format!("{dir}/{d}")));
        models.extend(
            l.files
                .iter()
                .filter(|f| f.ends_with(".pmd"))
                .map(|f| format!("{dir}/{f}")),
        );
    }
    let (mut ccw, mut cw) = (0u64, 0u64);
    let mut effects: BTreeMap<String, usize> = BTreeMap::new();
    let mut mat_errors = 0;
    for path in models.iter().step_by(7) {
        let Ok(model) = cache.load(&fs, path) else {
            continue;
        };
        for piece in &model.pmg.pieces {
            for t in piece.indices.chunks_exact(3) {
                let [a, b, c] = [t[0], t[1], t[2]].map(|i| piece.positions[i as usize]);
                let n = piece.normals[t[0] as usize];
                let (u, v) = (sub(b, a), sub(c, a));
                let face = [
                    u[1] * v[2] - u[2] * v[1],
                    u[2] * v[0] - u[0] * v[2],
                    u[0] * v[1] - u[1] * v[0],
                ];
                let d = face[0] * n[0] + face[1] * n[1] + face[2] * n[2];
                if d > 0.0 {
                    ccw += 1
                } else if d < 0.0 {
                    cw += 1
                }
            }
        }
        for look in &model.pmd.looks {
            for m in &look.materials {
                match fs.read(m).ok().and_then(|b| Material::parse(&b).ok()) {
                    Some(mat) => *effects.entry(mat.effect).or_default() += 1,
                    None => mat_errors += 1,
                }
            }
        }
    }
    println!(
        "models sampled: {}, triangles ccw-vs-normal {ccw}, cw {cw}, material errors {mat_errors}",
        models.len() / 7
    );
    let mut sorted: Vec<_> = effects.into_iter().collect();
    sorted.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    for (e, n) in sorted.iter().take(60) {
        println!("{n:>6} {e}");
    }
    Ok(())
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
