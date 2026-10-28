//! Research tool for docs/formats/models.md, "Piece": what the PMG piece
//! `u32` at +8 (`Piece::uv_routing`) encodes. For every `.pmd`/`.pmg` pair
//! under the given roots (default `vehicle/truck`), it tallies the stored
//! value against the piece's UV set count and against the effect of the
//! piece's material in every look, and checks the reading "nibble n (from
//! the least significant) names the stored UV set that feeds UV slot n;
//! 0xf leaves the slot unused". It also lists every per-texture and
//! material attribute key of the `.mat` files used, to see whether a
//! material names a UV set itself.
//! Given `.pmd` paths instead of roots, prints every piece's stored value,
//! effects and the u/v range of each stored UV set.
//! Usage: ETS2_DIR=... cargo run --release -p specsheet-research --example uv_routing_survey [-- <root> ... | <model.pmd> ...]

use std::collections::{BTreeMap, BTreeSet};

use specsheet_archive::Vfs;
use specsheet_data::{render::paint_uv_set, GameInstall};
use specsheet_formats::{mat::Material, pmd::Pmd, pmg::Pmg};

/// Effect name tokens that name a texture with its own UV routing in the
/// `dif.spec` and `truckpaint` specs.
const FLAVOURS: &[&str] = &[
    "tsnmap",
    "tsnmapuv",
    "tsnmapuv2",
    "oclu",
    "truckpaint",
    "altuv",
    "flipflakeuv",
    "detail",
    "decal",
    "lightmap",
    "mult",
];

/// The 8 nibbles of `routing`, slot 0 first.
fn nibbles(routing: u32) -> [u8; 8] {
    std::array::from_fn(|n| ((routing >> (4 * n)) & 0xf) as u8)
}

/// `slot:set` for every slot whose nibble is not 0xf.
fn describe(routing: u32) -> String {
    let used: Vec<String> = nibbles(routing)
        .iter()
        .enumerate()
        .filter(|(_, &set)| set != 0xf)
        .map(|(slot, set)| format!("{slot}:{set}"))
        .collect();
    if used.is_empty() {
        "-".to_owned()
    } else {
        used.join(" ")
    }
}

struct MatInfo {
    effect: String,
    texture_keys: Vec<(String, String)>,
    attribute_keys: Vec<String>,
}

fn load(vfs: &Vfs, path: &str) -> Option<MatInfo> {
    let material = vfs
        .read(path.trim_start_matches('/'))
        .ok()
        .and_then(|b| Material::parse(&b).ok())?;
    Some(MatInfo {
        effect: material
            .effect
            .trim_start_matches("eut2.")
            .trim_end_matches(".rfx")
            .to_owned(),
        texture_keys: material
            .textures
            .iter()
            .flat_map(|t| {
                t.attributes
                    .iter()
                    .map(|(k, _)| (t.name.clone(), k.clone()))
            })
            .collect(),
        attribute_keys: material.attributes.iter().map(|(k, _)| k.clone()).collect(),
    })
}

fn flavours(effect: &str) -> String {
    let tokens: BTreeSet<&str> = effect.split('.').collect();
    let present: Vec<&str> = FLAVOURS
        .iter()
        .copied()
        .filter(|f| tokens.contains(f))
        .collect();
    format!(
        "{} [{}]",
        effect.split('.').next().unwrap_or(""),
        present.join(" ")
    )
}

/// A per-piece rule: holds / fails, with the first failures.
#[derive(Default)]
struct Rule {
    holds: usize,
    fails: usize,
    examples: Vec<String>,
}

impl Rule {
    fn check(&mut self, ok: bool, example: impl FnOnce() -> String) {
        if ok {
            self.holds += 1;
        } else {
            self.fails += 1;
            if self.examples.len() < 12 {
                self.examples.push(example());
            }
        }
    }
}

/// Slot `n`'s stored set, or `None` for 0xf.
fn slot(routing: u32, n: usize) -> Option<usize> {
    let set = nibbles(routing)[n];
    (set != 0xf).then_some(usize::from(set))
}

#[derive(Default)]
struct Tally {
    pieces: usize,
    example: String,
}

fn bump<K: Ord>(map: &mut BTreeMap<K, Tally>, key: K, example: &str) {
    let tally = map.entry(key).or_default();
    tally.pieces += 1;
    if tally.example.is_empty() {
        tally.example = example.to_owned();
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (vfs, _) = GameInstall::discover()?.mount()?;
    let mut roots: Vec<String> = std::env::args().skip(1).collect();
    if roots.is_empty() {
        roots.push("vehicle/truck".to_owned());
    }
    if roots.iter().all(|r| r.ends_with(".pmd")) {
        return per_piece(&vfs, &roots);
    }
    let mut models = Vec::new();
    let mut stack = roots.clone();
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

    let mut mats: BTreeMap<String, Option<MatInfo>> = BTreeMap::new();
    let mut by_count: BTreeMap<(usize, u32), Tally> = BTreeMap::new();
    let mut by_flavour: BTreeMap<(String, String), Tally> = BTreeMap::new();
    let mut by_effect: BTreeMap<(String, String), Tally> = BTreeMap::new();
    let mut looks_disagree = 0usize;
    let mut set_out_of_range = Vec::new();
    let mut nibble_values: BTreeMap<(usize, u8), usize> = BTreeMap::new();
    let mut pieces_total = 0usize;
    let mut rules: BTreeMap<&str, Rule> = BTreeMap::new();
    // (stored value, sets, altuv in a .mat) -> (mask agrees, alt agrees, pieces)
    let mut paint: BTreeMap<(u32, usize), [usize; 3]> = BTreeMap::new();
    // (slot, routed set) -> pieces whose routed set is byte-identical to set 0
    let mut same_as_base: BTreeMap<(usize, usize), [usize; 2]> = BTreeMap::new();
    let mut unreadable = 0usize;
    for model in &models {
        let Ok(Ok(pmd)) = vfs.read(model).map(|b| Pmd::parse(&b)) else {
            unreadable += 1;
            continue;
        };
        let Ok(Ok(pmg)) = vfs
            .read(&model.replace(".pmd", ".pmg"))
            .map(|b| Pmg::parse(&b))
        else {
            unreadable += 1;
            continue;
        };
        for (index, piece) in pmg.pieces.iter().enumerate() {
            pieces_total += 1;
            let sets = piece.uv_sets.len();
            let example = format!("{model} piece {index}");
            bump(&mut by_count, (sets, piece.uv_routing), &example);
            for (slot, set) in nibbles(piece.uv_routing).into_iter().enumerate() {
                *nibble_values.entry((slot, set)).or_default() += 1;
                if set != 0xf && usize::from(set) >= sets && set_out_of_range.len() < 20 {
                    set_out_of_range.push(format!(
                        "{example}: {:#010x} with {sets} sets",
                        piece.uv_routing
                    ));
                }
            }
            let mut effects = BTreeSet::new();
            for look in &pmd.looks {
                let Some(path) = look.materials.get(piece.material as usize) else {
                    continue;
                };
                let info = mats.entry(path.clone()).or_insert_with(|| load(&vfs, path));
                if let Some(info) = info {
                    effects.insert(info.effect.clone());
                }
            }
            let tokens: BTreeSet<&str> = effects.iter().flat_map(|e| e.split('.')).collect();
            let families: BTreeSet<&str> =
                effects.iter().filter_map(|e| e.split('.').next()).collect();
            let r = piece.uv_routing;
            let used: Vec<usize> = (0..8).filter_map(|n| slot(r, n)).collect();
            let what = || format!("{example}: {r:#010x}, {sets} sets, {effects:?}");
            if !effects.is_empty() {
                rules
                    .entry("slot 6 used <=> a look has tsnmapuv or tsnmapuv2")
                    .or_default()
                    .check(
                        slot(r, 6).is_some()
                            == (tokens.contains("tsnmapuv") || tokens.contains("tsnmapuv2")),
                        what,
                    );
                rules
                    .entry("slot 2 used <=> a look is truckpaint")
                    .or_default()
                    .check(
                        slot(r, 2).is_some() == families.contains("truckpaint"),
                        what,
                    );
                rules
                    .entry("slot 1 used <=> a look has oclu, mult, lamp or truckpaint")
                    .or_default()
                    .check(
                        slot(r, 1).is_some()
                            == (tokens.contains("oclu")
                                || tokens.contains("mult")
                                || families.contains("lamp")
                                || families.contains("truckpaint")),
                        what,
                    );
                rules
                    .entry("slot 0 used <=> the piece stores UV sets")
                    .or_default()
                    .check(slot(r, 0).is_some() == (sets > 0), what);
            }
            rules
                .entry("sets == 1 + highest routed set (0 if none)")
                .or_default()
                .check(sets == used.iter().max().map_or(0, |m| m + 1), what);
            rules
                .entry("every stored set is routed to some slot")
                .or_default()
                .check((0..sets).all(|set| used.contains(&set)), what);
            rules
                .entry("slots 3, 4, 5, 7 unused")
                .or_default()
                .check([3, 4, 5, 7].iter().all(|&n| slot(r, n).is_none()), what);
            if families.contains("truckpaint") {
                let row = paint.entry((r, sets)).or_default();
                row[0] += usize::from(paint_uv_set(sets, false) == slot(r, 1));
                row[1] += usize::from(paint_uv_set(sets, true) == slot(r, 2));
                row[2] += 1;
            }
            for (n, set) in (0..8).filter_map(|n| slot(r, n).map(|set| (n, set))) {
                if set == 0 || set >= sets {
                    continue;
                }
                let row = same_as_base.entry((n, set)).or_default();
                row[0] += usize::from(piece.uv_sets[set] == piece.uv_sets[0]);
                row[1] += 1;
            }
            let flavour_set: BTreeSet<String> = effects.iter().map(|e| flavours(e)).collect();
            if flavour_set.len() > 1 {
                looks_disagree += 1;
            }
            let routing = describe(piece.uv_routing);
            for flavour in flavour_set {
                bump(&mut by_flavour, (flavour, routing.clone()), &example);
            }
            for effect in effects {
                bump(&mut by_effect, (effect, routing.clone()), &example);
            }
        }
    }

    println!(
        "roots {roots:?}: {} models ({unreadable} unreadable), {pieces_total} pieces",
        models.len()
    );
    println!("\n== stored value by UV set count");
    println!("pieces sets value       slot:set  e.g.");
    for ((sets, value), t) in &by_count {
        println!(
            "{:6} {sets:4} {value:#010x}  {:9} {}",
            t.pieces,
            describe(*value),
            t.example
        );
    }
    println!("\n== nibble values per slot (slot 0 = least significant nibble)");
    for slot in 0..8 {
        let row: Vec<String> = nibble_values
            .iter()
            .filter(|((s, _), _)| *s == slot)
            .map(|((_, v), n)| format!("{v:x}:{n}"))
            .collect();
        println!("slot {slot}: {}", row.join(" "));
    }
    println!(
        "\npieces naming a set >= their UV set count: {}",
        set_out_of_range.len()
    );
    for line in &set_out_of_range {
        println!("  {line}");
    }
    println!("pieces whose looks' effects differ in flavour: {looks_disagree}");
    println!(
        "
== per-piece rules (effects of every look; rules on effects skip pieces with no readable material)"
    );
    for (name, rule) in &rules {
        println!("{name}: holds {}, fails {}", rule.holds, rule.fails);
        for line in &rule.examples {
            println!("    {line}");
        }
    }
    println!(
        "
== truckpaint pieces: specsheet_data::render::paint_uv_set vs slots 1 and 2"
    );
    println!(
        "value       sets slot:set              pieces mask=slot1 alt=slot2 (rule mask, rule alt)"
    );
    let (mut mask_ok, mut alt_ok, mut all) = (0, 0, 0);
    for ((value, sets), [mask, alt, n]) in &paint {
        mask_ok += mask;
        alt_ok += alt;
        all += n;
        println!(
            "{value:#010x} {sets:4} {:22} {n:6} {mask:9} {alt:9} ({:?}, {:?})",
            describe(*value),
            paint_uv_set(*sets, false),
            paint_uv_set(*sets, true)
        );
    }
    println!("total {all}: mask agrees {mask_ok}, alt agrees {alt_ok}");
    println!(
        "
== routed sets other than 0: pieces whose set equals set 0 byte for byte"
    );
    for ((n, set), [same, total]) in &same_as_base {
        println!("slot {n} set {set}: {same} of {total} identical to set 0");
    }
    println!("\n== routing by effect family and flavour tokens (all looks)");
    println!("pieces routing  family [flavours]  e.g.");
    for ((flavour, routing), t) in &by_flavour {
        println!("{:6} {routing:12} {flavour}  {}", t.pieces, t.example);
    }
    println!("\n== routing by full effect name (all looks)");
    for ((effect, routing), t) in &by_effect {
        println!("{:6} {routing:12} {effect}  {}", t.pieces, t.example);
    }

    let mut texture_keys: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut attribute_keys: BTreeMap<String, usize> = BTreeMap::new();
    for info in mats.values().flatten() {
        for key in &info.texture_keys {
            *texture_keys.entry(key.clone()).or_default() += 1;
        }
        for key in &info.attribute_keys {
            *attribute_keys.entry(key.clone()).or_default() += 1;
        }
    }
    println!(
        "\n== .mat keys over {} materials ({} unreadable)",
        mats.len(),
        mats.values().filter(|m| m.is_none()).count()
    );
    println!("per-texture attribute keys (slot.key: materials):");
    for ((slot, key), n) in &texture_keys {
        println!("  {slot}.{key}: {n}");
    }
    println!("material attribute keys (key: materials):");
    for (key, n) in &attribute_keys {
        println!("  {key}: {n}");
    }
    Ok(())
}

/// Prints, for each piece of `models`, the stored value, the effect of its
/// material in every look and the min/max of every stored UV set.
fn per_piece(vfs: &Vfs, models: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    for model in models {
        let path = model.trim_start_matches('/');
        let pmd = Pmd::parse(&vfs.read(path)?)?;
        let pmg = Pmg::parse(&vfs.read(&path.replace(".pmd", ".pmg"))?)?;
        println!("== {model}");
        for (index, piece) in pmg.pieces.iter().enumerate() {
            let effects: BTreeSet<String> = pmd
                .looks
                .iter()
                .filter_map(|look| look.materials.get(piece.material as usize))
                .filter_map(|m| load(vfs, m).map(|info| info.effect))
                .collect();
            println!(
                "piece {index}: {:#010x} ({}) {} sets {effects:?}",
                piece.uv_routing,
                describe(piece.uv_routing),
                piece.uv_sets.len()
            );
            for (set, uvs) in piece.uv_sets.iter().enumerate() {
                let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
                for uv in uvs {
                    for axis in 0..2 {
                        lo[axis] = lo[axis].min(uv[axis]);
                        hi[axis] = hi[axis].max(uv[axis]);
                    }
                }
                println!(
                    "  set {set}: u {:.3}..{:.3}  v {:.3}..{:.3}",
                    lo[0], hi[0], lo[1], hi[1]
                );
            }
        }
    }
    Ok(())
}
