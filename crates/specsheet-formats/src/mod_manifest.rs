//! `manifest.sii` of a mod package, written as SII text (D18). Attributes
//! and their meaning: `docs/formats/mods.md`.
//!
//! Only attributes seen in real manifests are written. The unit name is
//! dot-prefixed (`mod_package : .name`), the form 28 of 31 ZIP manifests
//! use.

use std::fmt::Write;

/// Errors produced while building a manifest.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ManifestError {
    #[error("unit name `{0}` is not 1 to 12 characters of `a-z`, `0-9`, `_`")]
    InvalidUnitName(String),
}

/// The contents of a `mod_package` unit.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModManifest {
    /// The unit name without its leading dot.
    pub unit_name: String,
    pub package_version: String,
    pub display_name: String,
    pub author: String,
    /// Mod-manager categories; the game falls back to defaults for invalid
    /// ones.
    pub categories: Vec<String>,
    /// Root-relative icon file (`.jpg`), if any.
    pub icon: Option<String>,
    /// Root-relative description file (`.txt`), if any.
    pub description_file: Option<String>,
    /// Game-version globs such as `1.60.*`. Empty means compatible with
    /// every version.
    pub compatible_versions: Vec<String>,
}

impl ModManifest {
    /// The manifest as SII text.
    pub fn to_sii(&self) -> Result<String, ManifestError> {
        // A name token is a u64 in base 38, so 12 characters at most.
        let name_ok = (1..=12).contains(&self.unit_name.len())
            && self
                .unit_name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
        if !name_ok {
            return Err(ManifestError::InvalidUnitName(self.unit_name.clone()));
        }
        let mut out = String::from("SiiNunit\n{\n");
        let _ = writeln!(out, "mod_package : .{}\n{{", self.unit_name);
        attribute(&mut out, "package_version", &self.package_version);
        attribute(&mut out, "display_name", &self.display_name);
        attribute(&mut out, "author", &self.author);
        for category in &self.categories {
            attribute(&mut out, "category[]", category);
        }
        if let Some(icon) = &self.icon {
            attribute(&mut out, "icon", icon);
        }
        if let Some(file) = &self.description_file {
            attribute(&mut out, "description_file", file);
        }
        for version in &self.compatible_versions {
            attribute(&mut out, "compatible_versions[]", version);
        }
        out.push_str("}\n}\n");
        Ok(out)
    }
}

fn attribute(out: &mut String, key: &str, value: &str) {
    let _ = writeln!(out, "\t{key}: {}", quote(value));
}

/// A quoted SII string. Every byte outside printable ASCII, and `"` and `\`,
/// is written as `\xNN`, the escape the game itself writes for UTF-8.
fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for byte in value.bytes() {
        if byte.is_ascii_graphic() && byte != b'"' && byte != b'\\' || byte == b' ' {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "\\x{byte:02x}");
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sii;

    fn sample() -> ModManifest {
        ModManifest {
            unit_name: "test_mod".to_owned(),
            package_version: "1.0".to_owned(),
            display_name: "Test \"mod\" \\ 5\u{20ac}\nline".to_owned(),
            author: "Someone".to_owned(),
            categories: vec!["truck".to_owned(), "tuning_parts".to_owned()],
            icon: Some("icon.jpg".to_owned()),
            description_file: Some("description.txt".to_owned()),
            compatible_versions: vec!["1.60.*".to_owned()],
        }
    }

    #[test]
    fn writes_a_manifest_the_sii_parser_reads_back() {
        let manifest = sample();
        let text = manifest.to_sii().unwrap();
        assert!(text.is_ascii());
        let document = sii::parse("/manifest.sii", text.as_bytes()).unwrap();
        assert_eq!(document.units.len(), 1);
        let unit = &document.units[0];
        assert_eq!(unit.class, "mod_package");
        assert_eq!(unit.name, ".test_mod");
        assert_eq!(unit.get_str("package_version"), Some("1.0"));
        assert_eq!(unit.get_str("display_name"), Some(&*manifest.display_name));
        assert_eq!(unit.get_str("author"), Some("Someone"));
        assert_eq!(unit.array_str("category"), ["truck", "tuning_parts"]);
        assert_eq!(unit.get_str("icon"), Some("icon.jpg"));
        assert_eq!(unit.get_str("description_file"), Some("description.txt"));
        assert_eq!(unit.array_str("compatible_versions"), ["1.60.*"]);
    }

    #[test]
    fn escapes_quotes_backslashes_and_non_ascii_as_hex_bytes() {
        assert_eq!(quote("a \"b\" \\"), r#""a \x22b\x22 \x5c""#);
        assert_eq!(quote("5\u{20ac}\n"), r#""5\xe2\x82\xac\x0a""#);
    }

    #[test]
    fn leaves_out_optional_attributes() {
        let manifest = ModManifest {
            unit_name: "m".to_owned(),
            ..ModManifest::default()
        };
        let text = manifest.to_sii().unwrap();
        for absent in [
            "category",
            "icon",
            "description_file",
            "compatible_versions",
        ] {
            assert!(!text.contains(absent), "{absent}");
        }
        assert!(text.contains("display_name: \"\""));
    }

    #[test]
    fn rejects_unit_names_that_are_not_tokens() {
        for name in ["", "Test", "my-mod", "a.b", "thirteen_char"] {
            let manifest = ModManifest {
                unit_name: name.to_owned(),
                ..ModManifest::default()
            };
            assert_eq!(
                manifest.to_sii(),
                Err(ManifestError::InvalidUnitName(name.to_owned())),
                "{name:?}"
            );
        }
    }
}
