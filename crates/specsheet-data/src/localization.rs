//! Display strings from `/locale/<language>/local*.sii` (`localization_db`
//! units with parallel `key[]` / `val[]` arrays). Definitions reference
//! them as `@@key@@`.

use std::collections::HashMap;

use crate::error::Result;
use crate::fs::GameFs;

/// The game's display strings for one language, by localisation key.
#[derive(Clone, Debug, Default)]
pub struct Localization {
    strings: HashMap<String, String>,
}

impl Localization {
    /// Loads every `local*.sii` of `language` (e.g. `en_us`). Files that
    /// fail to parse are skipped; their keys simply stay unresolved.
    pub fn load(fs: &GameFs, language: &str) -> Result<Self> {
        let dir = format!("/locale/{language}");
        let mut strings = HashMap::new();
        for file in fs.list_dir(&dir)?.files {
            if !(file.starts_with("local") && file.ends_with(".sii")) {
                continue;
            }
            let Ok(doc) = fs.read_sii(&format!("{dir}/{file}")) else {
                continue;
            };
            for unit in doc.units_of_class("localization_db") {
                let keys = unit.array_str("key");
                let values = unit.array_str("val");
                strings.extend(
                    keys.into_iter()
                        .zip(values)
                        .map(|(k, v)| (k.to_owned(), v.to_owned())),
                );
            }
        }
        Ok(Self { strings })
    }

    /// The string of `key` (without the `@@` marks).
    pub fn get(&self, key: &str) -> Option<&str> {
        self.strings.get(key).map(String::as_str)
    }

    /// Replaces every `@@key@@` in `text`; unknown keys are left as the bare
    /// key so the UI still shows something meaningful.
    pub fn resolve(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(start) = rest.find("@@") {
            out.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            let Some(end) = after.find("@@") else {
                out.push_str(&rest[start..]);
                return out;
            };
            let key = &after[..end];
            out.push_str(self.get(key).unwrap_or(key));
            rest = &after[end + 2..];
        }
        out.push_str(rest);
        out
    }

    /// Number of strings.
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    /// Whether no string was loaded.
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::Localization;

    #[test]
    fn resolves_keys_and_keeps_unknown_ones_readable() {
        let loc = Localization {
            strings: [("mirror_chromed".to_owned(), "Chromed".to_owned())].into(),
        };
        assert_eq!(loc.resolve("@@mirror_chromed@@"), "Chromed");
        assert_eq!(loc.resolve("A @@mirror_chromed@@ B"), "A Chromed B");
        assert_eq!(loc.resolve("@@missing_key@@"), "missing_key");
        assert_eq!(loc.resolve("plain"), "plain");
        assert_eq!(loc.resolve("broken @@tail"), "broken @@tail");
    }
}
