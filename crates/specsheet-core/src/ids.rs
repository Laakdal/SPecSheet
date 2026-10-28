//! Strongly typed identifiers. All wrap the exact strings used by game data.

use std::fmt;

use serde::{Deserialize, Serialize};

macro_rules! string_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Wraps the game's string unchanged.
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            /// The game's string.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({:?})", stringify!($name), self.0)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }
    };
}

string_id!(
    /// A truck model, e.g. `scania.s_2016` (the `/def/vehicle/truck/<id>` directory).
    TruckId
);
string_id!(
    /// An accessory slot, e.g. `chassis`, `cabin` or `s_mirror`.
    SlotId
);
string_id!(
    /// An accessory unit name, e.g. `stock.scania.s_2016.s_mirror`.
    AccessoryId
);

impl AccessoryId {
    /// Whether this id matches `pattern`, an accessory id in which `*`
    /// matches any run of characters (`*.renault.etech_t.chassis`).
    pub fn matches(&self, pattern: &AccessoryId) -> bool {
        glob_match(pattern.as_str(), self.as_str())
    }
}

fn glob_match(pattern: &str, text: &str) -> bool {
    let Some((first, rest)) = pattern.split_once('*') else {
        return pattern == text;
    };
    let Some(mut remaining) = text.strip_prefix(first) else {
        return false;
    };
    let mut pieces: Vec<&str> = rest.split('*').collect();
    let last = pieces.pop().unwrap_or("");
    for piece in pieces {
        match remaining.find(piece) {
            Some(at) => remaining = &remaining[at + piece.len()..],
            None => return false,
        }
    }
    remaining.len() >= last.len() && remaining.ends_with(last)
}

#[cfg(test)]
mod tests {
    use super::AccessoryId;

    #[test]
    fn wildcard_patterns_match_like_globs() {
        let id = AccessoryId::from("4x2.renault.etech_t.chassis");
        assert!(id.matches(&"*.renault.etech_t.chassis".into()));
        assert!(id.matches(&"4x2.renault.etech_t.chassis".into()));
        assert!(id.matches(&"*".into()));
        assert!(id.matches(&"4x2.*.chassis".into()));
        assert!(!id.matches(&"*.renault.t.chassis".into()));
        assert!(!id.matches(&"6x2.*".into()));
        assert!(!AccessoryId::from("ab").matches(&"a*b*b".into()));
    }
}
