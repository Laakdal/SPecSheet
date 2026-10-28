//! Game data layer: finds the ETS2 installation, mounts its archives and
//! turns game files into the domain types of `specsheet-core` and into
//! renderer-independent meshes. Never writes to the installation.

pub mod assembly;
mod catalog;
mod error;
mod fs;
mod install;
pub mod localization;
pub mod model;
pub mod paint;
pub mod pins;
pub mod render;
pub mod scene;
pub mod session;
pub mod slots;
pub mod ui;
pub mod view;

pub use catalog::{
    list_trucks, load_catalog, load_preset, load_vehicle_config, LoadWarning, TruckSummary,
};
pub use error::{GameDataError, Result};
pub use fs::GameFs;
pub use install::{parse_library_folders, GameInstall, BASE_MOUNT_ORDER, TESTED_GAME_VERSION};
