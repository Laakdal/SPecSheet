//! Game data layer: finds the ETS2 installation, mounts its archives and
//! turns game files into the domain types of `specsheet-truck` and into
//! renderer-independent meshes. Never writes to the installation. Assembly,
//! the game's UI files and the tuning session build on it in their own
//! crates (`specsheet-assembly`, `specsheet-gameui`, `specsheet-session`).

mod catalog;
mod error;
mod fs;
mod install;
pub mod localization;
pub mod mod_names;
pub mod model;
pub mod paint;
pub mod render;
pub mod scene;

pub use catalog::{
    list_trucks, load_catalog, load_preset, load_vehicle_config, LoadWarning, TruckSummary,
};
pub use error::{GameDataError, Result};
pub use fs::GameFs;
pub use install::{parse_library_folders, GameInstall, BASE_MOUNT_ORDER, TESTED_GAME_VERSION};
