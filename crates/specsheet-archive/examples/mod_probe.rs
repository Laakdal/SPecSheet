//! Research tool: what the mod folder's packages are made of.
//!
//! Usage:
//!   cargo run --release --example mod_probe -- <mod dir>
//!
//! Sniffs every file by content (not extension) and reads it with the
//! matching reader: `Archive` (HashFS v2), `HashFsV1Archive` (HashFS v1) or
//! `ZipArchive`. For v1 and ZIP every entry is read, to check sizes and
//! compression across the whole package. Files are opened read-only.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use specsheet_archive::{Archive, HashFsV1Archive, ZipArchive};

fn u16_le(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn probe_v1(path: &Path) -> Result<(), String> {
    let archive = HashFsV1Archive::open(path).map_err(|e| e.to_string())?;
    let mut failed = Vec::new();
    let (mut dirs, mut listed, mut compressed, mut bytes) = (0, 0, 0, 0u64);
    let mut flag_kinds: Vec<u32> = Vec::new();
    for entry in archive.entries() {
        if !flag_kinds.contains(&entry.flags) {
            flag_kinds.push(entry.flags);
        }
        if entry.is_compressed() {
            compressed += 1;
        }
        match archive.read_entry(entry) {
            Ok(data) => {
                bytes += data.len() as u64;
                if entry.is_directory() {
                    dirs += 1;
                    // A listing is text: every line a name or `*`subdir.
                    if std::str::from_utf8(&data).is_ok() {
                        listed += 1;
                    }
                }
            }
            Err(e) => failed.push(format!("{:#018x}: {e}", entry.hash)),
        }
    }
    flag_kinds.sort_unstable();
    println!(
        "    v1 salt={:#06x} entries={} flags={flag_kinds:?} compressed={compressed} dirs={dirs} utf8_listings={listed} read_ok={} failed={} bytes={bytes} root_listing={}",
        archive.salt(),
        archive.len(),
        archive.len() - failed.len(),
        failed.len(),
        archive.list_dir("").is_ok()
    );
    for f in failed.iter().take(5) {
        println!("    FAILED {f}");
    }
    match archive.read("manifest.sii") {
        Ok(bytes) => print_manifest_fields(&bytes),
        Err(e) => println!("    manifest.sii: {e}"),
    }
    Ok(())
}

fn probe_zip(path: &Path) -> Result<(), String> {
    let zip = ZipArchive::open(path).map_err(|e| e.to_string())?;
    let mut failed = Vec::new();
    let mut bytes = 0u64;
    let mut names: Vec<&str> = zip.files().collect();
    names.sort_unstable();
    for name in &names {
        match zip.read(name) {
            Ok(data) => bytes += data.len() as u64,
            Err(e) => failed.push(format!("{name}: {e}")),
        }
    }
    println!(
        "    zip records={} files={} read_ok={} failed={} bytes={bytes}",
        zip.record_count(),
        zip.len(),
        names.len() - failed.len(),
        failed.len()
    );
    for f in failed.iter().take(5) {
        println!("    FAILED {f}");
    }
    match zip.read("manifest.sii") {
        Ok(bytes) => print_manifest_fields(&bytes),
        Err(e) => println!("    manifest.sii: {e}"),
    }
    Ok(())
}

fn probe_v2(path: &Path) -> Result<(), String> {
    let archive = Archive::open(path).map_err(|e| e.to_string())?;
    println!("    v2 opened by Archive: {} entries", archive.len());
    match archive.list_dir("") {
        Ok(root) => println!(
            "    root dirs={:?} files={:?}",
            root.directories, root.files
        ),
        Err(e) => println!("    root listing: {e}"),
    }
    match archive.read("manifest.sii") {
        Ok(bytes) => print_manifest_fields(&bytes),
        Err(e) => println!("    manifest.sii: {e}"),
    }
    Ok(())
}

/// Prints attribute names only (and the version-gating values), not the
/// third-party text.
fn print_manifest_fields(bytes: &[u8]) {
    let text = String::from_utf8_lossy(bytes);
    let mut fields: Vec<String> = Vec::new();
    let mut compat: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() || key.contains(' ') {
            continue;
        }
        if key.starts_with("compatible_versions") {
            compat.push(value.trim().to_string());
        }
        if !fields.iter().any(|f| f == key) {
            fields.push(key.to_string());
        }
    }
    println!("    manifest fields: {fields:?}");
    if !compat.is_empty() {
        println!("    compatible_versions: {compat:?}");
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::args()
        .nth(1)
        .ok_or("usage: mod_probe <mod dir>")?;
    let mut names: Vec<_> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|e| e.path())
        .collect();
    names.sort();
    for path in names {
        let mut header = [0u8; 48];
        let n = File::open(&path)?.read(&mut header)?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let kind = match (&header[..4.min(n)], n >= 6) {
            (b"SCS#", true) => format!("hashfs v{}", u16_le(&header, 4)),
            (b"PK\x03\x04", _) => "zip".to_string(),
            _ => "unknown".to_string(),
        };
        println!("{name}: {kind}");
        let result = match kind.as_str() {
            "hashfs v2" => probe_v2(&path),
            "hashfs v1" => probe_v1(&path),
            "zip" => probe_zip(&path),
            _ => Ok(()),
        };
        if let Err(e) = result {
            println!("    ERROR: {e}");
        }
    }
    Ok(())
}
