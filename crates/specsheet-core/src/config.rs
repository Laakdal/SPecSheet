//! Truck configuration state and the rules that keep it consistent.
//!
//! Rules (evidence in `docs/TUNING_SYSTEM.md`):
//! * `suitable_for[]` — an accessory is allowed only while at least one of
//!   the listed accessories is selected. Entries may contain `*` wildcards.
//! * `conflict_with[]` — an accessory is not allowed while any listed
//!   accessory is selected (checked in both directions).
//! * `defaults[]` — selecting an accessory also selects these definitions,
//!   unless the slot already holds a compatible choice.
//! * `require[]` (truck data) — these slots must never be empty.
//! * Addons with an exterior model need a matching locator on the
//!   assembled base models ([`SlotAvailability`]); interior-only addons are
//!   not checked against exterior locators.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::catalog::{Accessory, TruckCatalog};
use crate::ids::{AccessoryId, SlotId, TruckId};

/// Which addon slots the current base models can host. Computed outside the
/// core (it needs model geometry); base/non-visual slots are always available.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotAvailability {
    pub available: BTreeSet<SlotId>,
}

/// Why an accessory cannot be selected right now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Incompatibility {
    /// None of `suitable_for` is selected.
    NotSuitable { needs_one_of: Vec<AccessoryId> },
    /// Conflicts with a selected accessory.
    ConflictsWith { accessory: AccessoryId },
    /// The base models have no locator for this slot.
    SlotUnavailable { slot: SlotId },
}

/// Why a hookup cannot go at a host position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HookupProblem {
    /// The host slot is empty.
    NoHost,
    /// The hookup is not in the catalog.
    UnknownHookup,
    /// The hookup does not list the host slot in `suitable_for[]`.
    NotSuitable,
    /// The host accessory's model has no such position.
    UnknownPosition,
}

/// A problem with a configuration as a whole.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConfigIssue {
    MissingRequiredSlot {
        slot: SlotId,
    },
    UnknownAccessory {
        accessory: AccessoryId,
    },
    Incompatible {
        accessory: AccessoryId,
        reason: Incompatibility,
    },
    InvalidHookup {
        host: SlotId,
        position: String,
        hookup: AccessoryId,
        problem: HookupProblem,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    #[error("accessory {0} does not exist for this truck")]
    UnknownAccessory(AccessoryId),
    #[error("accessory {accessory} is not compatible: {reason:?}")]
    Incompatible {
        accessory: AccessoryId,
        reason: Incompatibility,
    },
    #[error("slot {0} is required and cannot be emptied")]
    RequiredSlot(SlotId),
    #[error("hookup {hookup:?} cannot go at {host}/{position}: {problem:?}")]
    Hookup {
        host: SlotId,
        position: String,
        hookup: Option<AccessoryId>,
        problem: HookupProblem,
    },
}

/// What a change did, so callers can refresh only what changed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeSet {
    /// Slots whose selection changed (set, replaced or cleared).
    pub changed_slots: BTreeSet<SlotId>,
}

/// The selected accessory for every filled slot of one truck.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Configuration {
    truck: TruckId,
    selections: BTreeMap<SlotId, AccessoryId>,
    /// Body colour chosen for the paint job; `None` uses the paint job's
    /// own `base_color`.
    paint_color: Option<[f32; 3]>,
    /// Hookups per host slot, keyed by host-model position (`slot_NN`).
    #[serde(default)]
    hookups: BTreeMap<SlotId, BTreeMap<String, AccessoryId>>,
}

impl Configuration {
    pub fn new(truck: TruckId) -> Self {
        Self {
            truck,
            selections: BTreeMap::new(),
            paint_color: None,
            hookups: BTreeMap::new(),
        }
    }

    /// Builds a configuration from raw selections (e.g. a game preset)
    /// without applying any rule; use [`Configuration::validate`] after.
    pub fn from_selections(
        truck: TruckId,
        selections: impl IntoIterator<Item = (SlotId, AccessoryId)>,
    ) -> Self {
        Self {
            truck,
            selections: selections.into_iter().collect(),
            paint_color: None,
            hookups: BTreeMap::new(),
        }
    }

    pub fn set_paint_color(&mut self, color: Option<[f32; 3]>) {
        self.paint_color = color;
    }

    /// The effective body colour: the override, else the selected paint
    /// job's `base_color`.
    pub fn paint_color(&self, catalog: &TruckCatalog) -> Option<[f32; 3]> {
        self.paint_color.or_else(|| {
            self.selections
                .values()
                .filter_map(|id| catalog.accessory(id))
                .find_map(|a| a.base_color)
        })
    }

    /// Sets hookups without checking any rule (e.g. loading game configs);
    /// use [`Configuration::validate`] after.
    pub fn with_hookups(
        mut self,
        hookups: impl IntoIterator<Item = (SlotId, String, AccessoryId)>,
    ) -> Self {
        for (host, position, hookup) in hookups {
            self.hookups
                .entry(host)
                .or_default()
                .insert(position, hookup);
        }
        self
    }

    /// Hookups mounted on the accessory in `host`, by position.
    pub fn hookups(&self, host: &SlotId) -> Option<&BTreeMap<String, AccessoryId>> {
        self.hookups.get(host)
    }

    /// Every mounted hookup as `(host slot, position, hookup)`.
    pub fn all_hookups(&self) -> impl Iterator<Item = (&SlotId, &str, &AccessoryId)> {
        self.hookups.iter().flat_map(|(host, positions)| {
            positions
                .iter()
                .map(move |(position, id)| (host, position.as_str(), id))
        })
    }

    /// Mounts `hookup` at `position` of the accessory in `host`, or empties
    /// the position when `hookup` is `None`. `positions` are the positions
    /// the host accessory's model offers (its `slot_NN` locators).
    pub fn set_hookup(
        &mut self,
        catalog: &TruckCatalog,
        host: &SlotId,
        position: &str,
        positions: &[String],
        hookup: Option<&AccessoryId>,
    ) -> Result<ChangeSet, ConfigError> {
        let error = |problem| ConfigError::Hookup {
            host: host.clone(),
            position: position.to_owned(),
            hookup: hookup.cloned(),
            problem,
        };
        if !self.selections.contains_key(host) {
            return Err(error(HookupProblem::NoHost));
        }
        if !positions.iter().any(|p| p == position) {
            return Err(error(HookupProblem::UnknownPosition));
        }
        match hookup {
            Some(id) => {
                let found = catalog
                    .hookup(id)
                    .ok_or_else(|| error(HookupProblem::UnknownHookup))?;
                if !found.fits(host) {
                    return Err(error(HookupProblem::NotSuitable));
                }
                self.hookups
                    .entry(host.clone())
                    .or_default()
                    .insert(position.to_owned(), id.clone());
            }
            None => {
                if let Some(mounted) = self.hookups.get_mut(host) {
                    mounted.remove(position);
                    if mounted.is_empty() {
                        self.hookups.remove(host);
                    }
                }
            }
        }
        Ok(ChangeSet {
            changed_slots: [host.clone()].into(),
        })
    }

    pub fn truck(&self) -> &TruckId {
        &self.truck
    }

    pub fn selections(&self) -> &BTreeMap<SlotId, AccessoryId> {
        &self.selections
    }

    pub fn selected(&self, slot: &SlotId) -> Option<&AccessoryId> {
        self.selections.get(slot)
    }

    pub fn is_selected(&self, accessory: &AccessoryId) -> bool {
        self.selections.values().any(|a| a == accessory)
    }

    /// Checks whether `accessory` could occupy its slot given every other
    /// current selection (the accessory currently in that slot is ignored).
    pub fn check(
        &self,
        catalog: &TruckCatalog,
        availability: &SlotAvailability,
        accessory: &Accessory,
    ) -> Result<(), Incompatibility> {
        let others: Vec<&Accessory> = self
            .selections
            .iter()
            .filter(|(slot, _)| **slot != accessory.slot)
            .filter_map(|(_, id)| catalog.accessory(id))
            .collect();

        if accessory.needs_exterior_locator() && !availability.available.contains(&accessory.slot) {
            return Err(Incompatibility::SlotUnavailable {
                slot: accessory.slot.clone(),
            });
        }
        if !accessory.suitable_for.is_empty()
            && !others
                .iter()
                .any(|o| accessory.suitable_for.iter().any(|p| o.id.matches(p)))
        {
            return Err(Incompatibility::NotSuitable {
                needs_one_of: accessory.suitable_for.clone(),
            });
        }
        if let Some(conflict) = others.iter().find(|o| {
            accessory.conflict_with.iter().any(|p| o.id.matches(p))
                || o.conflict_with.iter().any(|p| accessory.id.matches(p))
        }) {
            return Err(Incompatibility::ConflictsWith {
                accessory: conflict.id.clone(),
            });
        }
        Ok(())
    }

    /// Selects `id` and repairs the rest of the configuration: applies the
    /// accessory's `defaults[]`, then replaces or clears selections that the
    /// change made incompatible (see [`Configuration::repair`]).
    ///
    /// `availability_for` recomputes slot availability for a candidate
    /// configuration; it is called again after base parts change.
    pub fn select(
        &mut self,
        catalog: &TruckCatalog,
        availability_for: &dyn Fn(&Configuration) -> SlotAvailability,
        id: &AccessoryId,
    ) -> Result<ChangeSet, ConfigError> {
        let accessory = catalog
            .accessory(id)
            .ok_or_else(|| ConfigError::UnknownAccessory(id.clone()))?;

        // The candidate must fit the rest of the configuration. Its own
        // rules are checked; parts it invalidates are repaired below (this
        // is how a chassis change replaces an unsuitable cabin).
        self.check(catalog, &availability_for(self), accessory)
            .map_err(|reason| ConfigError::Incompatible {
                accessory: id.clone(),
                reason,
            })?;

        let before = self.selections.clone();
        self.selections.insert(accessory.slot.clone(), id.clone());
        if accessory.kind == crate::catalog::AccessoryKind::PaintJob {
            // A new paint job brings its own colour.
            self.paint_color = None;
        }
        for default in &accessory.defaults {
            if let Some(default) = catalog.by_def_path(default) {
                let keep_current = self
                    .selections
                    .get(&default.slot)
                    .and_then(|current| catalog.accessory(current))
                    .is_some_and(|current| {
                        self.check(catalog, &availability_for(self), current)
                            .is_ok()
                    });
                if !keep_current && default.slot != accessory.slot {
                    self.selections
                        .insert(default.slot.clone(), default.id.clone());
                }
            }
        }
        self.repair(catalog, availability_for, &accessory.slot);
        let changes = diff(&before, &self.selections);
        self.drop_hookups_of(&changes);
        Ok(changes)
    }

    /// Empties `slot`. Fails for required slots.
    pub fn clear(
        &mut self,
        catalog: &TruckCatalog,
        availability_for: &dyn Fn(&Configuration) -> SlotAvailability,
        slot: &SlotId,
    ) -> Result<ChangeSet, ConfigError> {
        if self.required_slots(catalog).contains(slot) {
            return Err(ConfigError::RequiredSlot(slot.clone()));
        }
        let before = self.selections.clone();
        self.selections.remove(slot);
        self.repair(catalog, availability_for, slot);
        let changes = diff(&before, &self.selections);
        self.drop_hookups_of(&changes);
        Ok(changes)
    }

    /// Hookups are mounted on a specific host model's positions, so they are
    /// removed when the host accessory changes or is removed.
    fn drop_hookups_of(&mut self, changes: &ChangeSet) {
        for slot in &changes.changed_slots {
            self.hookups.remove(slot);
        }
    }

    /// Slots that must stay filled: the truck's `require[]` plus the
    /// `requires` of every selected accessory, plus the base slots.
    pub fn required_slots(&self, catalog: &TruckCatalog) -> BTreeSet<SlotId> {
        let mut required: BTreeSet<SlotId> = catalog.required_slots.iter().cloned().collect();
        required.insert(SlotId::from(crate::slots::CHASSIS));
        required.insert(SlotId::from(crate::slots::CABIN));
        for id in self.selections.values() {
            if let Some(accessory) = catalog.accessory(id) {
                required.extend(accessory.requires.iter().cloned());
            }
        }
        required
    }

    /// Makes the configuration consistent again, keeping `pinned` as is:
    /// every other incompatible selection is replaced by the first
    /// compatible candidate (fallbacks first, then catalog order) or
    /// cleared; empty required slots are filled the same way. Iterates
    /// because base-part changes alter slot availability.
    fn repair(
        &mut self,
        catalog: &TruckCatalog,
        availability_for: &dyn Fn(&Configuration) -> SlotAvailability,
        pinned: &SlotId,
    ) {
        // Each pass can only change slots other than `pinned`; the bound
        // guards against oscillating rule sets in malformed data.
        for _ in 0..8 {
            let availability = availability_for(self);
            let mut changed = false;

            let broken: Vec<SlotId> = self
                .selections
                .iter()
                .filter(|(slot, _)| *slot != pinned)
                .filter(|(_, id)| {
                    catalog
                        .accessory(id)
                        .is_none_or(|a| self.check(catalog, &availability, a).is_err())
                })
                .map(|(slot, _)| slot.clone())
                .collect();
            for slot in broken {
                self.selections.remove(&slot);
                changed = true;
            }

            for slot in self.required_slots(catalog) {
                if self.selections.contains_key(&slot) {
                    continue;
                }
                if let Some(candidate) = self.first_compatible(catalog, &availability, &slot) {
                    self.selections.insert(slot, candidate);
                    changed = true;
                }
            }
            if !changed {
                return;
            }
        }
    }

    /// First compatible accessory for `slot`: truck fallbacks, then the
    /// defaults of selected accessories, then any accessory of the slot.
    fn first_compatible(
        &self,
        catalog: &TruckCatalog,
        availability: &SlotAvailability,
        slot: &SlotId,
    ) -> Option<AccessoryId> {
        let selected_defaults = self
            .selections
            .values()
            .filter_map(|id| catalog.accessory(id))
            .flat_map(|a| a.defaults.iter());
        let preferred = catalog
            .fallbacks
            .iter()
            .chain(selected_defaults)
            .filter_map(|path| catalog.by_def_path(path))
            .filter(|a| &a.slot == slot);
        preferred
            .chain(catalog.in_slot(slot))
            .find(|a| self.check(catalog, availability, a).is_ok())
            .map(|a| a.id.clone())
    }

    /// Every rule violation of the configuration.
    pub fn validate(
        &self,
        catalog: &TruckCatalog,
        availability: &SlotAvailability,
    ) -> Vec<ConfigIssue> {
        let mut issues = Vec::new();
        for id in self.selections.values() {
            match catalog.accessory(id) {
                None => issues.push(ConfigIssue::UnknownAccessory {
                    accessory: id.clone(),
                }),
                Some(accessory) => {
                    if let Err(reason) = self.check(catalog, availability, accessory) {
                        issues.push(ConfigIssue::Incompatible {
                            accessory: id.clone(),
                            reason,
                        });
                    }
                }
            }
        }
        for slot in self.required_slots(catalog) {
            if !self.selections.contains_key(&slot) {
                issues.push(ConfigIssue::MissingRequiredSlot { slot });
            }
        }
        for (host, position, id) in self.all_hookups() {
            let problem = if !self.selections.contains_key(host) {
                Some(HookupProblem::NoHost)
            } else {
                match catalog.hookup(id) {
                    None => Some(HookupProblem::UnknownHookup),
                    Some(hookup) if !hookup.fits(host) => Some(HookupProblem::NotSuitable),
                    Some(_) => None,
                }
            };
            if let Some(problem) = problem {
                issues.push(ConfigIssue::InvalidHookup {
                    host: host.clone(),
                    position: position.to_owned(),
                    hookup: id.clone(),
                    problem,
                });
            }
        }
        issues
    }
}

fn diff(
    before: &BTreeMap<SlotId, AccessoryId>,
    after: &BTreeMap<SlotId, AccessoryId>,
) -> ChangeSet {
    let changed_slots = before
        .keys()
        .chain(after.keys())
        .filter(|slot| before.get(*slot) != after.get(*slot))
        .cloned()
        .collect();
    ChangeSet { changed_slots }
}
