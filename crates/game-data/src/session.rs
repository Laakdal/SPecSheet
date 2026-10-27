//! Application-facing orchestration: one mounted installation
//! ([`GameSession`]) and the truck currently being tuned ([`TruckSession`]).
//! UI layers translate these into their own transport types.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use tuning_core::{
    Accessory, AccessoryId, ChangeSet, ConfigError, Configuration, Hookup, Incompatibility, SlotId,
    TruckCatalog, TruckId,
};

use crate::assembly::{assemble, slot_availability, Assembly, Drive, HookupPosition};
use crate::catalog::{list_trucks, load_catalog, load_preset, LoadWarning, TruckSummary};
use crate::error::Result;
use crate::fs::GameFs;
use crate::install::GameInstall;
use crate::localization::Localization;
use crate::model::ModelCache;
use crate::scene::{list_scenes, Scene};

const LANGUAGE: &str = "en_us";

/// A mounted installation with shared caches.
#[derive(Debug)]
pub struct GameSession {
    install: GameInstall,
    fs: GameFs,
    models: ModelCache,
    localization: Localization,
    trucks: Vec<TruckSummary>,
    catalogs: Mutex<HashMap<TruckId, Arc<TruckCatalog>>>,
    /// Archives that failed to open (reported, not fatal).
    pub mount_failures: Vec<String>,
}

impl GameSession {
    /// Mounts `install` and reads the truck list and English strings.
    pub fn open(install: GameInstall) -> Result<Self> {
        let (vfs, failures) = install.mount()?;
        let fs = GameFs::new(vfs);
        let localization = Localization::load(&fs, LANGUAGE).unwrap_or_default();
        let trucks = list_trucks(&fs)?;
        Ok(Self {
            install,
            fs,
            models: ModelCache::new(),
            localization,
            trucks,
            catalogs: Mutex::new(HashMap::new()),
            mount_failures: failures
                .into_iter()
                .map(|(path, error)| format!("{}: {error}", path.display()))
                .collect(),
        })
    }

    pub fn install(&self) -> &GameInstall {
        &self.install
    }

    pub fn fs(&self) -> &GameFs {
        &self.fs
    }

    pub fn models(&self) -> &ModelCache {
        &self.models
    }

    pub fn localization(&self) -> &Localization {
        &self.localization
    }

    pub fn trucks(&self) -> &[TruckSummary] {
        &self.trucks
    }

    /// The game's tuning scenes, default garage first.
    pub fn scenes(&self) -> Result<Vec<Scene>> {
        list_scenes(&self.fs)
    }

    /// The catalog of `truck`, loaded once.
    pub fn catalog(&self, truck: &TruckId) -> Result<Arc<TruckCatalog>> {
        if let Some(catalog) = self.lock_catalogs().get(truck) {
            return Ok(Arc::clone(catalog));
        }
        let (catalog, _warnings) = load_catalog(&self.fs, truck)?;
        let catalog = Arc::new(catalog);
        self.lock_catalogs()
            .insert(truck.clone(), Arc::clone(&catalog));
        Ok(catalog)
    }

    /// Starts tuning `truck` from the game's preset, repaired by the rules.
    pub fn start_truck(&self, truck: &TruckId) -> Result<TruckSession> {
        let catalog = self.catalog(truck)?;
        let (config, warnings) = match load_preset(&self.fs, &catalog)? {
            Some(preset) => preset,
            None => (Configuration::new(truck.clone()), Vec::new()),
        };
        let mut session = TruckSession {
            catalog,
            config,
            drive: Drive::Left,
            assembly: Assembly::default(),
            load_warnings: warnings,
        };
        session.reassemble(self);
        Ok(session)
    }

    fn lock_catalogs(&self) -> std::sync::MutexGuard<'_, HashMap<TruckId, Arc<TruckCatalog>>> {
        self.catalogs.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// One option of a slot, with the rules' verdict for the current state.
#[derive(Debug)]
pub struct SlotOption<'a> {
    pub accessory: &'a Accessory,
    pub selected: bool,
    pub verdict: Result<(), Incompatibility>,
}

/// The truck being tuned: catalog, configuration and current assembly.
#[derive(Debug)]
pub struct TruckSession {
    catalog: Arc<TruckCatalog>,
    config: Configuration,
    drive: Drive,
    assembly: Assembly,
    load_warnings: Vec<LoadWarning>,
}

impl TruckSession {
    pub fn catalog(&self) -> &TruckCatalog {
        &self.catalog
    }

    pub fn config(&self) -> &Configuration {
        &self.config
    }

    pub fn drive(&self) -> Drive {
        self.drive
    }

    pub fn assembly(&self) -> &Assembly {
        &self.assembly
    }

    pub fn load_warnings(&self) -> &[LoadWarning] {
        &self.load_warnings
    }

    /// Every accessory of `slot` with its compatibility verdict, selected
    /// first, then compatible ones, then the rest; ties by price.
    pub fn options(&self, slot: &SlotId) -> Vec<SlotOption<'_>> {
        let selected = self.config.selected(slot);
        let mut options: Vec<SlotOption<'_>> = self
            .catalog
            .in_slot(slot)
            .map(|accessory| SlotOption {
                accessory,
                selected: selected == Some(&accessory.id),
                verdict: self
                    .config
                    .check(&self.catalog, &self.assembly.availability, accessory),
            })
            .collect();
        options.sort_by_key(|o| {
            (
                !o.selected,
                o.verdict.is_err(),
                o.accessory.price.unwrap_or(0),
            )
        });
        options
    }

    /// Selects `accessory`, applying the rules, and re-assembles the truck.
    pub fn select(
        &mut self,
        game: &GameSession,
        accessory: &AccessoryId,
    ) -> Result<ChangeSet, ConfigError> {
        let availability = |config: &Configuration| {
            slot_availability(game.fs(), game.models(), &self.catalog, config, self.drive)
        };
        let changes = self
            .config
            .select(&self.catalog, &availability, accessory)?;
        self.reassemble(game);
        Ok(changes)
    }

    /// Hookups that fit on the accessory in `host` (accessories mounted on
    /// accessories, finding F11), and the positions its model offers.
    pub fn hookup_options(&self, host: &SlotId) -> (Vec<&Hookup>, &[HookupPosition]) {
        let positions = self
            .assembly
            .hookup_positions
            .get(host)
            .map_or(&[][..], Vec::as_slice);
        if positions.is_empty() {
            return (Vec::new(), positions);
        }
        (self.catalog.hookups_for(host).collect(), positions)
    }

    /// Mounts `hookup` at `position` of the accessory in `host`, or empties
    /// the position, and re-assembles the truck.
    pub fn set_hookup(
        &mut self,
        game: &GameSession,
        host: &SlotId,
        position: &str,
        hookup: Option<&AccessoryId>,
    ) -> Result<ChangeSet, ConfigError> {
        let positions: Vec<String> = self
            .assembly
            .hookup_positions
            .get(host)
            .map(|p| p.iter().map(|p| p.name.clone()).collect())
            .unwrap_or_default();
        let changes = self
            .config
            .set_hookup(&self.catalog, host, position, &positions, hookup)?;
        self.reassemble(game);
        Ok(changes)
    }

    /// Switches between left- and right-hand drive models.
    pub fn set_drive(&mut self, game: &GameSession, drive: Drive) {
        self.drive = drive;
        self.reassemble(game);
    }

    fn reassemble(&mut self, game: &GameSession) {
        self.assembly = assemble(
            game.fs(),
            game.models(),
            &self.catalog,
            &self.config,
            self.drive,
        );
    }
}
