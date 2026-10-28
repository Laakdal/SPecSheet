//! Addon hookups: small parts (aux lights, beacons, horns, dashboard items)
//! mounted on another accessory rather than on the truck (finding F11).
//!
//! A hookup lists the host slots it fits (`suitable_for[]: "r_grill"`).
//! The host accessory's model exposes numbered positions (`slot_NN`
//! locators); a configuration maps positions of a host slot to hookups.

use serde::{Deserialize, Serialize};

use specsheet_core::{AccessoryId, SlotId};

/// One mountable hookup unit (`accessory_*hookup*_data : <id>.addon_hookup`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hookup {
    pub id: AccessoryId,
    /// Localisation key or literal display name.
    pub name: String,
    pub icon: Option<String>,
    pub price: Option<i64>,
    pub unlock_level: Option<i64>,
    /// Model path, placed at the host position's locator.
    pub model: Option<String>,
    /// Host slots this hookup can be mounted on.
    pub hosts: Vec<SlotId>,
    /// Interior hookup (dashboard, glass, cup holder items).
    pub interior: bool,
}

impl Hookup {
    pub fn fits(&self, host: &SlotId) -> bool {
        self.hosts.contains(host)
    }
}
