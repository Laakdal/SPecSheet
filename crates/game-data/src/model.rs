//! Loading and caching of PMD/PMG model pairs.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use scs_formats::pmd::Pmd;
use scs_formats::pmg::{Locator, Pmg};

use crate::error::{GameDataError, Result};
use crate::fs::GameFs;

/// A parsed model: descriptor plus geometry.
#[derive(Debug)]
pub struct Model {
    pub path: String,
    pub pmd: Pmd,
    pub pmg: Pmg,
}

impl Model {
    /// Index of `variant` (case-insensitive), falling back to the first.
    pub fn variant_index(&self, variant: Option<&str>) -> usize {
        variant
            .and_then(|name| self.pmd.variant_index(name))
            .unwrap_or(0)
    }

    /// Locators of the parts visible in `variant`.
    pub fn visible_locators(&self, variant: usize) -> impl Iterator<Item = &Locator> {
        self.pmg
            .parts
            .iter()
            .enumerate()
            .filter(move |(index, _)| self.pmd.is_part_visible(variant, *index))
            .flat_map(|(_, part)| &self.pmg.locators[part.locators.clone()])
    }
}

/// Parses each model at most once. Shared between threads.
#[derive(Debug, Default)]
pub struct ModelCache {
    models: Mutex<HashMap<String, Arc<Model>>>,
}

impl ModelCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads `path` (`.pmd`) and its sibling `.pmg`.
    pub fn load(&self, fs: &GameFs, path: &str) -> Result<Arc<Model>> {
        if let Some(model) = self.lock().get(path) {
            return Ok(Arc::clone(model));
        }
        // Parse outside the lock; a concurrent duplicate parse is harmless.
        let pmd = Pmd::parse(&fs.read(path)?).map_err(|source| GameDataError::Format {
            path: path.to_owned(),
            source,
        })?;
        let pmg_path = pmg_path(path);
        let pmg = Pmg::parse(&fs.read(&pmg_path)?).map_err(|source| GameDataError::Format {
            path: pmg_path,
            source,
        })?;
        let model = Arc::new(Model {
            path: path.to_owned(),
            pmd,
            pmg,
        });
        self.lock().insert(path.to_owned(), Arc::clone(&model));
        Ok(model)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Arc<Model>>> {
        // The map stays consistent even if a holder panicked.
        self.models.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn pmg_path(pmd_path: &str) -> String {
    match pmd_path.strip_suffix(".pmd") {
        Some(stem) => format!("{stem}.pmg"),
        None => format!("{pmd_path}.pmg"),
    }
}
