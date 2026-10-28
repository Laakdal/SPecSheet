//! Game paths that a file mentions, found by scanning its bytes. Used to
//! recover the names of files in HashFS v1 mod packages that ship without
//! directory listings (`docs/formats/mods.md`).
//!
//! Text and binary files alike store paths as plain ASCII: SII values
//! (`"/def/x.sii"`, `"engine|/def/x.soundref"`), `.soundref` lines
//! (`source="/sound/x.bank#event"`), and the material and texture paths
//! inside `.pmd`, `.mat` and `.tobj` files. Relative `@include` targets are
//! resolved against the including file.

use crate::sii::{self, resolve_relative};

/// Longest path we keep; game paths are far shorter.
const MAX_PATH: usize = 512;

/// Paths (without a leading slash, any `#event` suffix removed) that the
/// bytes of `file` mention. May contain paths that do not exist.
pub fn referenced_paths(file: &str, bytes: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    for run in bytes.split(|b| !(b.is_ascii_graphic() || *b == b' ' || *b == b'\t')) {
        let Ok(text) = std::str::from_utf8(run) else {
            continue;
        };
        for (at, _) in text.match_indices('/') {
            let before = text[..at].chars().next_back();
            let starts_path = before.is_none_or(|c| "\"|=:( \t,".contains(c));
            let next = text[at + 1..].chars().next();
            if starts_path && next.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') {
                push(&mut out, path_at(&text[at + 1..]));
            }
        }
        if let Some(rest) = text.trim_start().strip_prefix("@include") {
            if let Some(target) = rest
                .trim()
                .strip_prefix('"')
                .and_then(|r| r.split('"').next())
            {
                let file = format!("/{}", file.trim_start_matches('/'));
                push(
                    &mut out,
                    resolve_relative(&file, target).trim_start_matches('/'),
                );
            }
        }
        // A material names its texture beside it: `texture : "x.tobj"`
        // (docs/formats/ui.md, accessory icons).
        if file.ends_with(".mat") {
            let file = format!("/{}", file.trim_start_matches('/'));
            for quoted in text.split('"').skip(1).step_by(2) {
                if quoted.ends_with(".tobj") && !quoted.starts_with('/') {
                    push(
                        &mut out,
                        resolve_relative(&file, quoted).trim_start_matches('/'),
                    );
                }
            }
        }
    }
    out
}

/// Files that by convention sit beside `path` under the same stem
/// (counts in `research/evidence/mod-name-recovery.txt`):
/// - a model's geometry, `.pmd` → `.pmg` (all 70,076 game models);
/// - a texture's descriptor and material, `.dds` → `.tobj` (3,295 of 3,343
///   `.dds` in ZIP mods; the game itself packs textures into `.tobj`) and
///   `.tobj` → `.mat` (19,366 game textures);
/// - an FMOD bank's GUID list, `.bank` → `.bank.guids` (seen in a sound mod).
///
/// Guesses: callers keep only paths that exist.
pub fn companions(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (from, to) in [(".pmd", ".pmg"), (".dds", ".tobj"), (".tobj", ".mat")] {
        if let Some(stem) = path.strip_suffix(from) {
            out.push(format!("{stem}{to}"));
        }
    }
    if path.ends_with(".bank") {
        out.push(format!("{path}.guids"));
    }
    out
}

/// Files that the accessory units in SII `bytes` imply by convention:
/// - an `icon: "<p>"` is `material/ui/accessory/<p>.mat` (Confirmed,
///   docs/formats/ui.md);
/// - an addon unit `<name>.<truck>.<slot>` usually lives at
///   `def/vehicle/truck/<truck>/accessory/<slot>/<name>.sii` (76% of the
///   game's 29,841 addon units, `research/evidence/mod-name-recovery.txt`).
///
/// Guesses: callers keep only paths that exist.
pub fn unit_paths(bytes: &[u8]) -> Vec<String> {
    let Ok(document) = sii::parse("/unit_paths.sii", bytes) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for unit in &document.units {
        if let Some(icon) = unit
            .get_str("icon")
            .filter(|_| unit.class.starts_with("accessory_"))
        {
            push(&mut out, &format!("material/ui/accessory/{icon}.mat"));
        }
        let parts: Vec<&str> = unit.name.split('.').collect();
        if !unit.class.starts_with("accessory_addon") || parts.len() < 3 {
            continue;
        }
        let (name, rest) = (parts[0], &parts[1..]);
        let (slot, truck) = (rest[rest.len() - 1], rest[..rest.len() - 1].join("."));
        push(
            &mut out,
            &format!("def/vehicle/truck/{truck}/accessory/{slot}/{name}.sii"),
        );
    }
    out
}

/// The path characters at the start of `text`, cut at `#` (an event or
/// anchor inside the file).
fn path_at(text: &str) -> &str {
    let end = text
        .find(|c: char| !(c.is_ascii_alphanumeric() || "_-./".contains(c)))
        .unwrap_or(text.len());
    let path = &text[..end];
    path.split('#').next().unwrap_or(path)
}

/// `path` without a leading slash or trailing dots and slashes, if it is a
/// plausible game path: not empty, at most 512 bytes, and no empty, `.` or
/// `..` part.
pub fn clean(path: &str) -> Option<&str> {
    let path = path.trim_start_matches('/').trim_end_matches(['.', '/']);
    let sane = !path.is_empty()
        && path.len() <= MAX_PATH
        && !path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..");
    sane.then_some(path)
}

fn push(out: &mut Vec<String>, path: &str) {
    if let Some(path) = clean(path) {
        out.push(path.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_sii_soundref_and_binary_paths() {
        let sii = b"SiiNunit\n{\nx : .y {\n sounds[]: \"engine|/def/a/engine.soundref\"\n model: \"/vehicle/a/b.pmd\"\n}\n}\n";
        assert_eq!(
            referenced_paths("/def/a/x.sii", sii),
            ["def/a/engine.soundref", "vehicle/a/b.pmd"]
        );
        let soundref =
            b"\xef\xbb\xbfsource=\"/sound/truck/a.bank#dc16/engine\"\r\nposition_offset=0;0;0\r\n";
        assert_eq!(
            referenced_paths("/x.soundref", soundref),
            ["sound/truck/a.bank"]
        );
        // Paths inside binary data, between non-text bytes.
        let binary = b"\x01\x00\x10\x00/vehicle/a/b.mat\x00\x00\x07/vehicle/a/c.tobj\x00";
        assert_eq!(
            referenced_paths("/vehicle/a/b.pmd", binary),
            ["vehicle/a/b.mat", "vehicle/a/c.tobj"]
        );
    }

    #[test]
    fn resolves_relative_includes() {
        let sii = b"x : .y {\n @include \"sound.sui\"\n @include \"/def/shared.sui\"\n}\n";
        assert_eq!(
            referenced_paths("def/vehicle/truck/t/cabin.sii", sii),
            [
                "def/vehicle/truck/t/sound.sui",
                "def/shared.sui",
                "def/shared.sui"
            ]
        );
    }

    #[test]
    fn skips_comments_numbers_and_unsafe_paths() {
        let text = b"// comment\n/* block */\nratio: 3/4\nurl: \"http://example\"\nbad: \"/a/../b\"\nbad2: \"/a//b\"\n";
        assert!(referenced_paths("/x.sii", text).is_empty());
    }

    #[test]
    fn addon_units_give_their_conventional_file() {
        let sii = b"SiiNunit
{
accessory_addon_data : 530s_scs.scania.s_2016.badge
{
}
vehicle_accessory : .x
{
}
}
";
        assert_eq!(
            unit_paths(sii),
            ["def/vehicle/truck/scania.s_2016/accessory/badge/530s_scs.sii"]
        );
        assert!(unit_paths(b"not sii").is_empty());
        let with_icon =
            b"SiiNunit\n{\naccessory_addon_data : b.t.badge\n{\n icon: \"badges/max\"\n}\n}\n";
        assert_eq!(
            unit_paths(with_icon),
            [
                "material/ui/accessory/badges/max.mat",
                "def/vehicle/truck/t/accessory/badge/b.sii"
            ]
        );
    }

    #[test]
    fn materials_name_their_texture_beside_them() {
        let mat = b"material : \"ui\" {\n texture : \"max.tobj\"\n texture_name : \"texture\"\n}\n";
        assert_eq!(
            referenced_paths("material/ui/accessory/max.mat", mat),
            ["material/ui/accessory/max.tobj"]
        );
        // Outside a .mat, a bare quoted name is not a path.
        assert!(referenced_paths("def/x.sii", mat).is_empty());
    }

    #[test]
    fn clean_trims_and_rejects_unsafe_paths() {
        assert_eq!(clean("/def/a.sii"), Some("def/a.sii"));
        assert_eq!(clean("def/a/"), Some("def/a"));
        for bad in ["", "/", "a//b", "a/./b", "a/../b", "../a"] {
            assert_eq!(clean(bad), None, "{bad:?}");
        }
        assert_eq!(clean(&"a".repeat(MAX_PATH)).map(str::len), Some(MAX_PATH));
        assert_eq!(clean(&"a".repeat(MAX_PATH + 1)), None);
    }

    #[test]
    fn companions_follow_known_pairs_only() {
        assert_eq!(companions("vehicle/a/b.pmd"), ["vehicle/a/b.pmg"]);
        assert_eq!(companions("sound/a.bank"), ["sound/a.bank.guids"]);
        assert_eq!(companions("material/a.dds"), ["material/a.tobj"]);
        assert_eq!(companions("material/a.tobj"), ["material/a.mat"]);
        assert!(companions("def/a.sii").is_empty());
    }
}
