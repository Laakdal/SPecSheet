//! Hookups mounted on host accessories (F11).

use std::collections::BTreeMap;

use super::{ChangeSet, ConfigError, Configuration, HookupProblem};
use crate::catalog::TruckCatalog;
use specsheet_core::{AccessoryId, SlotId};

impl Configuration {
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
}
