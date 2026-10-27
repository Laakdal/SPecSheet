//! Truck tuning domain logic, independent of game file formats, rendering
//! and UI: the accessory catalog of a truck, compatibility rules and the
//! configuration state. Everything here is pure and unit-testable.

mod catalog;
mod config;
mod ids;
pub mod slots;

pub use catalog::{Accessory, AccessoryKind, ModelRef, TruckCatalog, WheelPart};
pub use config::{
    ChangeSet, ConfigError, ConfigIssue, Configuration, Incompatibility, SlotAvailability,
};
pub use ids::{AccessoryId, SlotId, TruckId};

#[cfg(test)]
mod tests;
