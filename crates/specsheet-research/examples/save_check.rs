//! Research tool: decode every `save/*/game.sii` under a profiles directory
//! and check binary SII against text SII. Files are only read. Output holds
//! class and attribute names and counts only, never values or paths.
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
        }
    }
    println!("{} saves: {forms:?}, {units} units decoded", files.len());
    for (name, stats) in &placements {
        println!("placement (8 floats) {name}: {stats:?}");
    }
    println!("nameless references in binary saves: {nameless}, resolving to a unit in the same file: {nameless_resolved}");

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
