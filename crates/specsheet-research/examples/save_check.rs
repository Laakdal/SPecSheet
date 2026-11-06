//! Research tool: decode every `save/*/game.sii` under a profiles directory
//! and check binary SII against text SII. Files are only read. Output holds
//! class and attribute names and counts only, never positions or paths. The
//! only values printed are the commonest binary values of attributes that
//! text writes as `nil`, to show its sentinel. It also tests how placement
//! float 3 packs the position (`research/evidence/bsii-placement-nil.txt`).
//!
//! Usage:
//!   cargo run --release -p specsheet-research --example save_check -- "<home>/profiles"
//!
//! Optional, to stderr: `SAVE_CHECK_F3=1` prints float 3 of every binary
//! placement; `SAVE_CHECK_KEY=<attribute>` prints that scalar from binary units.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use specsheet_formats::scsc;
use specsheet_formats::sii::{SiiDocument, Slot, Value};

/// What a value looks like in the text form.
fn kind(value: &Value) -> String {
    match value {
        Value::String(_) => "string".into(),
        Value::Tuple(items) => format!("tuple{}", items.len()),
        Value::Token(t) => {
            if t == "true" || t == "false" {
                "bool".into()
            } else if t.starts_with('&') || (t.contains('.') && t.parse::<f64>().is_ok()) {
                "float".into()
            } else if t.parse::<i64>().is_ok() || t.parse::<u64>().is_ok() {
                "int".into()
            } else {
                "name".into()
            }
        }
    }
}

type Shape = BTreeMap<(String, String), BTreeSet<String>>;

fn shape(doc: &SiiDocument, out: &mut Shape) {
    for unit in &doc.units {
        for attr in &unit.attributes {
            let prefix = match attr.slot {
                Slot::Scalar => "",
                _ => "[]",
            };
            out.entry((unit.class.clone(), attr.key.clone()))
                .or_default()
                .insert(format!("{prefix}{}", kind(&attr.value)));
        }
    }
}

/// Which slice of an 8-float placement is a unit quaternion, and what the
/// remaining float holds (counts only).
fn place_stats(doc: &SiiDocument, out: &mut BTreeMap<&'static str, usize>) {
    for unit in &doc.units {
        for attr in &unit.attributes {
            let Some(f) = attr.value.as_floats().filter(|f| f.len() == 8) else {
                continue;
            };
            let norm = |s: &[f32]| s.iter().map(|v| v * v).sum::<f32>().sqrt();
            *out.entry("total").or_default() += 1;
            if (norm(&f[3..7]) - 1.0).abs() < 1e-3 {
                *out.entry("quat at 3..7").or_default() += 1;
            }
            if (norm(&f[4..8]) - 1.0).abs() < 1e-3 {
                *out.entry("quat at 4..8").or_default() += 1;
            }
            if f[3] == 0.0 {
                *out.entry("f[3] == 0").or_default() += 1;
            }
            if f[7] == 0.0 {
                *out.entry("f[7] == 0").or_default() += 1;
            }
            if f[3].to_bits() != 0 && f[3].fract() == 0.0 {
                *out.entry("f[3] whole non-zero").or_default() += 1;
            }
            if std::env::var_os("SAVE_CHECK_F3").is_some() {
                eprintln!("f3 {}", f[3]);
            }
        }
    }
}

/// Floats 0..4 of every binary placement and the position of every text
/// placement (7 floats), to test how float 3 packs the position.
#[derive(Default)]
struct Positions {
    binary: Vec<[f32; 4]>,
    text: Vec<[f32; 3]>,
}

fn collect_positions(doc: &SiiDocument, binary: bool, out: &mut Positions) {
    for unit in &doc.units {
        for attr in &unit.attributes {
            let Some(f) = attr.value.as_floats() else {
                continue;
            };
            match (binary, f.len()) {
                (true, 8) => out.binary.push([f[0], f[1], f[2], f[3]]),
                (false, 7) => out.text.push([f[0], f[1], f[2]]),
                _ => {}
            }
        }
    }
}

/// Float 3 read as the whole number it holds, split into its high and low
/// 12-bit fields, each biased by 2048.
fn cell_fields(f3: f32) -> (i32, i32) {
    // Float 3 is a whole number below 2^24 in every sample, so the cast is
    // exact; anything else decodes to a field the match below rejects.
    let n = f3 as u32;
    (((n >> 12) & 0xfff) as i32 - 2048, (n & 0xfff) as i32 - 2048)
}

/// How many binary placements outside cell (0, 0) land within 0.1 m (and
/// 0.1 m in y) of some text placement when float 3's fields are read as
/// cell indices of `size` metres: `high_is_z` picks which field is z.
fn cell_matches(p: &Positions, size: f32, high_is_z: bool) -> usize {
    p.binary
        .iter()
        .filter(|b| b[1] < 1e10 && cell_fields(b[3]) != (0, 0))
        .filter(|b| {
            let (high, low) = cell_fields(b[3]);
            let (cx, cz) = if high_is_z { (low, high) } else { (high, low) };
            let (x, z) = (b[0] + cx as f32 * size, b[2] + cz as f32 * size);
            p.text
                .iter()
                .any(|t| (t[0] - x).hypot(t[2] - z) < 0.1 && (t[1] - b[1]).abs() < 0.1)
        })
        .count()
}

/// Whether a binary kind is consistent with the kinds seen in text: text
/// writes whole floats as plain integers (`0`), and null references as `null`.
fn compatible(binary: &str, text: &BTreeSet<String>) -> bool {
    let (prefix, base) = binary
        .strip_prefix("[]")
        .map_or(("", binary), |b| ("[]", b));
    let ok = |k: &str| text.contains(&format!("{prefix}{k}"));
    match base {
        "float" => ok("float") || ok("int"),
        "name" => ok("name") || ok("int") || ok("string"),
        other => ok(other),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("usage: save_check <profiles dir>")?,
    );
    let mut files = Vec::new();
    for profile in std::fs::read_dir(&root)?.flatten() {
        let Ok(saves) = std::fs::read_dir(profile.path().join("save")) else {
            continue;
        };
        for save in saves.flatten() {
            let game = save.path().join("game.sii");
            if game.is_file() {
                files.push(game);
            }
        }
    }
    files.sort();

    let (mut text_shape, mut binary_shape) = (Shape::new(), Shape::new());
    let mut forms: BTreeMap<String, usize> = BTreeMap::new();
    let (mut nameless, mut nameless_resolved, mut units) = (0usize, 0usize, 0usize);
    let mut placements: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut positions = Positions::default();
    // Attributes text writes as `nil`, and what binary holds for them.
    let mut text_nil: BTreeSet<(String, String)> = BTreeSet::new();
    let mut text_nil_count: BTreeMap<(String, String), (usize, usize)> = BTreeMap::new();
    let mut binary_scalars: Vec<(String, String, String)> = Vec::new();
    for (index, path) in files.iter().enumerate() {
        let bytes = std::fs::read(path)?;
        let container = if bytes.starts_with(b"ScsC") {
            "ScsC"
        } else {
            "plain"
        };
        let payload = if container == "ScsC" {
            scsc::decrypt(&bytes)?
        } else {
            bytes.clone()
        };
        let form = String::from_utf8_lossy(&payload[..4.min(payload.len())]).into_owned();
        *forms.entry(format!("{container} -> {form}")).or_default() += 1;
        let doc = match scsc::document(&format!("save{index:02}"), &bytes) {
            Ok(doc) => doc,
            Err(e) => {
                println!("save{index:02} ({container} -> {form}): ERROR {e}");
                continue;
            }
        };
        units += doc.units.len();
        if form == "BSII" {
            shape(&doc, &mut binary_shape);
            place_stats(&doc, &mut placements);
            collect_positions(&doc, true, &mut positions);
            for unit in &doc.units {
                for attr in &unit.attributes {
                    if let (Slot::Scalar, Value::Token(t)) = (&attr.slot, &attr.value) {
                        binary_scalars.push((unit.class.clone(), attr.key.clone(), t.clone()));
                    }
                }
            }
            if let Some(key) = std::env::var_os("SAVE_CHECK_KEY") {
                let key = key.to_string_lossy();
                for unit in &doc.units {
                    if let Some(v) = unit.get_str(&key) {
                        eprintln!("key {v}");
                    }
                }
            }
            let names: BTreeSet<&str> = doc.units.iter().map(|u| u.name.as_str()).collect();
            for unit in &doc.units {
                for attr in &unit.attributes {
                    if let Value::Token(t) = &attr.value {
                        if t.starts_with("_nameless.") {
                            nameless += 1;
                            if names.contains(t.as_str()) {
                                nameless_resolved += 1;
                            }
                        }
                    }
                }
            }
        } else {
            shape(&doc, &mut text_shape);
            collect_positions(&doc, false, &mut positions);
            for unit in &doc.units {
                for attr in &unit.attributes {
                    let id = (unit.class.clone(), attr.key.clone());
                    let nil = matches!(&attr.value, Value::Token(t) if t == "nil");
                    if nil {
                        text_nil.insert(id.clone());
                    }
                    if matches!(attr.slot, Slot::Scalar) {
                        let count = text_nil_count.entry(id).or_default();
                        count.0 += usize::from(nil);
                        count.1 += 1;
                    }
                }
            }
        }
    }
    println!("{} saves: {forms:?}, {units} units decoded", files.len());
    for (name, stats) in &placements {
        println!("placement (8 floats) {name}: {stats:?}");
    }
    println!("nameless references in binary saves: {nameless}, resolving to a unit in the same file: {nameless_resolved}");
    println!(
        "placement positions: {} binary, {} text; binary outside cell (0, 0) that land within 0.1 m of a text one:",
        positions.binary.len(),
        positions.text.len()
    );
    for high_is_z in [true, false] {
        let cells: Vec<String> = [256.0, 512.0, 1024.0]
            .iter()
            .map(|&size| format!("{size} m: {}", cell_matches(&positions, size, high_is_z)))
            .collect();
        let layout = if high_is_z {
            "high = z, low = x"
        } else {
            "high = x, low = z"
        };
        println!("  {layout}: {}", cells.join(", "));
    }
    // For each such attribute: how often text writes nil, and the most
    // common binary values (the nil sentinel should lead).
    let mut nil_values: BTreeMap<(String, String), BTreeMap<String, usize>> = BTreeMap::new();
    for (class, key, value) in binary_scalars {
        let id = (class, key);
        if text_nil.contains(&id) {
            *nil_values.entry(id).or_default().entry(value).or_default() += 1;
        }
    }
    println!(
        "attributes text writes as nil: {}; their commonest binary values:",
        text_nil.len()
    );
    for ((class, key), values) in &nil_values {
        let mut top: Vec<_> = values.iter().collect();
        top.sort_by(|a, b| b.1.cmp(a.1));
        let total: usize = values.values().sum();
        let shown: Vec<String> = top
            .iter()
            .take(3)
            .map(|(v, n)| format!("{v} x{n}"))
            .collect();
        println!("  {class}.{key} ({total}): {}", shown.join(", "));
    }
    println!(
        "text nil share per attribute: {:?}",
        text_nil_count
            .iter()
            .filter(|(_, (nil, _))| *nil > 0)
            .map(|((c, k), (nil, all))| format!("{c}.{k} {nil}/{all}"))
            .collect::<Vec<_>>()
    );

    let shared: Vec<_> = binary_shape
        .keys()
        .filter(|k| text_shape.contains_key(*k))
        .collect();
    let mut mismatches = Vec::new();
    for key in &shared {
        let text = &text_shape[*key];
        for b in &binary_shape[*key] {
            if !compatible(b, text) {
                mismatches.push(format!("{}.{}: binary {b}, text {text:?}", key.0, key.1));
            }
        }
    }
    println!(
        "attributes: {} in binary, {} in text, {} shared; {} kind mismatches",
        binary_shape.len(),
        text_shape.len(),
        shared.len(),
        mismatches.len()
    );
    for m in mismatches.iter().take(40) {
        println!("  {m}");
    }
    Ok(())
}
