//! Research tool: writes a test mod package that changes nothing in the
//! game (D18). It holds only `manifest.sii` and a description, so loading it
//! shows whether the game accepts our ZIP output and manifest. No game path
//! is overridden and no game file is copied.
//!
//! Writes a new `specsheet_test.scs` into `<dir>` and refuses when a package
//! of that name exists. Nothing else is written.
//!
//! Usage:
//!   cargo run --release -p specsheet-research --example mod_write -- <dir>

use specsheet_archive::{write_new_package, ZipArchive, ZipWriter};
use specsheet_formats::mod_manifest::ModManifest;
use specsheet_formats::sii;

const FILE_NAME: &str = "specsheet_test.scs";
const DESCRIPTION: &str = "Test package written by SPecSheet.\n\
    It holds only this description and its manifest, and changes nothing in the game.\n";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::args().nth(1).ok_or("usage: mod_write <dir>")?;
    let manifest = ModManifest {
        unit_name: "specsheet".to_owned(),
        package_version: "1".to_owned(),
        display_name: "SPecSheet test package".to_owned(),
        author: "SPecSheet".to_owned(),
        categories: vec!["other".to_owned()],
        icon: None,
        description_file: Some("description.txt".to_owned()),
        compatible_versions: vec!["1.60.*".to_owned()],
    }
    .to_sii()?;

    let mut writer = ZipWriter::new();
    writer.add("manifest.sii", manifest.as_bytes())?;
    writer.add("description.txt", DESCRIPTION.as_bytes())?;
    let bytes = writer.finish()?;
    let path = write_new_package(dir.as_ref(), FILE_NAME, &bytes)?;

    // Read it back with the same reader as the mods in the mod folder.
    let archive = ZipArchive::open(&path)?;
    let unit = sii::parse("/manifest.sii", &archive.read("manifest.sii")?)?
        .units_of_class("mod_package")
        .next()
        .map(|unit| unit.name.clone())
        .ok_or("no mod_package unit")?;
    println!(
        "wrote {} ({} bytes, {} files, unit {unit})",
        path.display(),
        bytes.len(),
        archive.len()
    );
    Ok(())
}
