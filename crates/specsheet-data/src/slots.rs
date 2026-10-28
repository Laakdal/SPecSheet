//! Slot ↔ locator matching (finding F4 in `docs/REVERSE_ENGINEERING.md`).
//!
//! Base models (chassis, cabin) carry locators named after the addon slots
//! they can host, sometimes with a suffix: `mirror_s`, `beacon_sl`,
//! `toybig_1`, `set_lglass3`, `doorsteps`. A locator belongs to a slot when
//! its name starts with the slot name; when several slots match, the
//! longest slot name wins, so `f_light` can never claim `f_light_top`.
//! Measured on all 26 game presets: 626 of 626 selected exterior addons
//! resolve (`research/evidence/locator-check-prefix.txt`). The engine's
//! own rule is not yet confirmed from the executable.

use std::collections::{BTreeMap, BTreeSet};

use specsheet_formats::pmg::Locator;
use specsheet_truck::SlotId;

/// Whether `locator` is named for `slot` under the matching rule.
pub fn matches_slot(slot: &str, locator: &str) -> bool {
    locator.starts_with(slot)
}

/// Assigns every locator to at most one of `slots` (the longest matching
/// slot name) and returns the locators per slot.
pub fn locators_by_slot<'a>(
    slots: &BTreeSet<SlotId>,
    locators: impl IntoIterator<Item = &'a Locator>,
) -> BTreeMap<SlotId, Vec<&'a Locator>> {
    let mut by_slot: BTreeMap<SlotId, Vec<&Locator>> = BTreeMap::new();
    for locator in locators {
        let best = slots
            .iter()
            .filter(|slot| matches_slot(slot.as_str(), &locator.name))
            .max_by_key(|slot| slot.as_str().len());
        if let Some(slot) = best {
            by_slot.entry(slot.clone()).or_default().push(locator);
        }
    }
    by_slot
}

#[cfg(test)]
mod tests {
    use super::*;

    fn locator(name: &str) -> Locator {
        Locator {
            name: name.to_owned(),
            position: [0.0; 3],
            scale: 1.0,
            rotation_wxyz: [1.0, 0.0, 0.0, 0.0],
            hookup: None,
        }
    }

    #[test]
    fn matches_exact_suffixed_and_numbered_names() {
        assert!(matches_slot("s_mirror", "s_mirror"));
        assert!(matches_slot("mirror", "mirror_s"));
        assert!(matches_slot("beacon", "beacon_sl"));
        assert!(matches_slot("toybig", "toybig_1"));
        assert!(matches_slot("set_lglass", "set_lglass3"));
        assert!(matches_slot("doorstep", "doorsteps"));
        assert!(!matches_slot("s_mirror", "mirror_s"));
    }

    #[test]
    fn longest_slot_name_wins() {
        let slots: BTreeSet<SlotId> = ["f_light", "f_light_top", "light"].map(SlotId::from).into();
        let locators = [locator("f_light_top"), locator("light_bar")];
        let by_slot = locators_by_slot(&slots, &locators);
        assert_eq!(by_slot[&SlotId::from("f_light_top")].len(), 1);
        assert!(!by_slot.contains_key(&SlotId::from("f_light")));
        assert_eq!(by_slot[&SlotId::from("light")][0].name, "light_bar");
    }
}
