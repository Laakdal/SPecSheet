//! Hookups mounted on their host accessory's `slot_NN` positions
//! (finding F11).

use std::collections::BTreeMap;

use specsheet_formats::pmg::Locator;
use specsheet_truck::{Configuration, SlotId, TruckCatalog};

use super::{Placement, Transform};
use specsheet_data::model::ModelCache;
use specsheet_data::GameFs;

/// Prefix of the hookup position locators on host accessory models.
const HOOKUP_LOCATOR: &str = "slot_";

/// A position on a host accessory where a hookup can be mounted.
#[derive(Clone, Debug, PartialEq)]
pub struct HookupPosition {
    /// The `slot_NN` locator name, as stored in configurations.
    pub name: String,
    /// Truck-space position (on the first placement of the host).
    pub position: [f32; 3],
}

pub(super) type HookupResult = (Option<Placement>, Option<String>);

/// The hookup positions of every placed host accessory, and the placements
/// of the configured hookups (or a warning per hookup that cannot be
/// placed). A hookup's transform is its host's placement composed with the
/// position locator.
pub(super) fn hookup_placements(
    fs: &GameFs,
    cache: &ModelCache,
    catalog: &TruckCatalog,
    config: &Configuration,
    placements: &[Placement],
) -> (BTreeMap<SlotId, Vec<HookupPosition>>, Vec<HookupResult>) {
    let mut positions = BTreeMap::new();
    let mut hosts: BTreeMap<&SlotId, Vec<(&Placement, Vec<Locator>)>> = BTreeMap::new();
    for placement in placements {
        let Ok(model) = cache.load(fs, &placement.model) else {
            continue;
        };
        let variant = model.variant_index(placement.variant.as_deref());
        let locators: Vec<Locator> = model
            .visible_locators(variant)
            .filter(|l| l.name.starts_with(HOOKUP_LOCATOR))
            .cloned()
            .collect();
        if locators.is_empty() {
            continue;
        }
        positions.entry(placement.slot.clone()).or_insert_with(|| {
            locators
                .iter()
                .map(|l| HookupPosition {
                    name: l.name.clone(),
                    position: placement
                        .transform
                        .compose(&Transform::from_locator(l))
                        .translation,
                })
                .collect()
        });
        hosts
            .entry(&placement.slot)
            .or_default()
            .push((placement, locators));
    }
    let mut results = Vec::new();
    for (host, position, id) in config.all_hookups() {
        let Some(model) = catalog.hookup(id).and_then(|h| h.model.as_ref()) else {
            results.push((None, Some(format!("hookup {id} has no model"))));
            continue;
        };
        let mut placed = false;
        for (host_placement, locators) in hosts.get(host).into_iter().flatten() {
            if let Some(locator) = locators.iter().find(|l| l.name == position) {
                placed = true;
                results.push((
                    Some(Placement {
                        slot: host.clone(),
                        accessory: id.clone(),
                        model: model.clone(),
                        look: None,
                        variant: None,
                        transform: host_placement
                            .transform
                            .compose(&Transform::from_locator(locator)),
                    }),
                    None,
                ));
            }
        }
        if !placed {
            results.push((
                None,
                Some(format!("hookup {id}: no position `{position}` on {host}")),
            ));
        }
    }
    (positions, results)
}
