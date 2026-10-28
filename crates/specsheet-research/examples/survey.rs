//! Research tool: walks every archive from its root listing, tallies entry
//! kinds per file extension and texture formats, and verifies that every
//! listed name resolves and that sample textures decompress and unpack.
//!
//! Usage: cargo run --release -p specsheet-research --example survey -- <archive.scs>...

use std::collections::{BTreeMap, VecDeque};

use specsheet_archive::{Archive, Content, EntryKind};

fn main() {
    for path in std::env::args().skip(1) {
        match Archive::open(&path) {
            Ok(archive) => survey(&archive),
            Err(e) => println!("{path}: ERROR {e}"),
        }
    }
}

fn survey(archive: &Archive) {
    let mut by_ext: BTreeMap<String, BTreeMap<&str, usize>> = BTreeMap::new();
    let mut formats: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
    let (mut reached, mut missing, mut texture_errors) = (0usize, 0usize, Vec::new());
    let mut queue = VecDeque::from([String::new()]);
    while let Some(dir) = queue.pop_front() {
        reached += 1;
        let Ok(listing) = archive.list_dir(&dir) else {
            continue;
        };
        let join = |name: &str| {
            if dir.is_empty() {
                name.to_owned()
            } else {
                format!("{dir}/{name}")
            }
        };
        for name in &listing.directories {
            let path = join(name);
            match archive.entry(&path) {
                Some(e) if e.kind == EntryKind::Directory => queue.push_back(path),
                _ => missing += 1,
            }
        }
        for name in &listing.files {
            let path = join(name);
            let Some(entry) = archive.entry(&path) else {
                missing += 1;
                continue;
            };
            reached += 1;
            let ext = name
                .rsplit_once('.')
                .map_or("<none>", |(_, e)| e)
                .to_owned();
            let kind = match &entry.content {
                Content::Stored(b) if b.compressed => "stored/zlib",
                Content::Stored(_) => "stored/raw",
                Content::Texture { image, .. } => {
                    let slot = formats.entry(image.dxgi_format).or_default();
                    slot.0 += 1;
                    if slot.1 < 3 {
                        slot.1 += 1;
                        if let Err(e) = archive
                            .read_texture(&path)
                            .map_err(|e| e.to_string())
                            .and_then(|t| t.unpack())
                        {
                            texture_errors.push(format!("{path}: {e}"));
                        }
                    }
                    "texture"
                }
            };
            *by_ext.entry(ext).or_default().entry(kind).or_default() += 1;
        }
    }
    println!(
        "{}: {} entries, {} reachable, {} listed-but-missing, {} skipped",
        archive.path().display(),
        archive.len(),
        reached,
        missing,
        archive.skipped_entries().len()
    );
    for (ext, kinds) in by_ext {
        println!("  .{ext}: {kinds:?}");
    }
    println!("  dxgi formats (count, sampled): {formats:?}");
    for e in texture_errors {
        println!("  TEXTURE ERROR {e}");
    }
    for s in archive.skipped_entries().iter().take(5) {
        println!("  SKIPPED {:#018x}: {}", s.hash, s.reason);
    }
}
