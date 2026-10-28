//! Truck tuning domain logic, independent of game file formats, rendering
//! and UI: the accessory catalog of a truck, compatibility rules and the
//! configuration state. Everything here is pure and unit-testable.

mod catalog;
mod config;
mod hookup;
mod ids;
mod paint;
pub mod slots;

pub use catalog::{Accessory, AccessoryKind, ModelRef, TruckCatalog, WheelPart};
pub use config::{
    ChangeSet, ConfigError, ConfigIssue, Configuration, HookupProblem, Incompatibility,
    SlotAvailability,
};
pub use hookup::Hookup;
pub use ids::{AccessoryId, SlotId, TruckId};
pub use paint::{
    shared_mask_color, srgb_to_linear, Flake, Flip, PaintJob, PaintLocks, PartPaint,
    DEFAULT_FLAKE_NOISE,
};

#[cfg(test)]
mod tests;
