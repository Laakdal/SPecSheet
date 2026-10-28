//! Research tool: the active mod list of a profile, encrypted (ScsC) or
//! already decrypted. Files are only read.
//!
//! Usage:
//!   cargo run --release -p specsheet-research --example profile_mods -- <profile.sii> ...

use specsheet_formats::{scsc, sii};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path)?;
        let container = String::from_utf8_lossy(&bytes[..bytes.len().min(4)]).into_owned();
        let text = scsc::sii_text(&bytes)?;
        let doc = sii::parse(&path, &text)?;
        let Some(profile) = doc.units_of_class("user_profile").next() else {
            println!("{path}: no user_profile unit");
            continue;
        };
        let mods = profile.array_str("active_mods");
        println!(
            "{path}: {container} -> {} bytes of SII text, {} active mods",
            text.len(),
            mods.len()
        );
        for (i, entry) in mods.iter().enumerate() {
            println!("  [{i}] {entry}");
        }
    }
    Ok(())
}
