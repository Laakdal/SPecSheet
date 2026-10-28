//! Truck tuning domain logic, independent of game file formats, rendering
//! and UI: the accessory catalog of a truck, compatibility rules and the
//! configuration state. Everything here is pure and unit-testable.

#![warn(missing_docs)]

mod catalog;
mod config;
mod hookup;
mod paint;
pub mod slots;

pub use catalog::{Accessory, AccessoryKind, ModelRef, TruckCatalog, WheelPart};
pub use config::{
    ChangeSet, ConfigError, ConfigIssue, Configuration, HookupProblem, Incompatibility,
    PaintOverride, SlotAvailability,
};
pub use hookup::Hookup;
pub use paint::{
    shared_mask_color, Flake, Flip, PaintJob, PaintLocks, PartPaint, DEFAULT_BASE_COLOR,
    DEFAULT_FLAKE_CLEARCOAT_ROLLOFF, DEFAULT_FLAKE_COLOR, DEFAULT_FLAKE_DENSITY,
    DEFAULT_FLAKE_NOISE, DEFAULT_FLAKE_SHININESS, DEFAULT_FLAKE_UVSCALE, DEFAULT_FLAKE_VRATIO,
    DEFAULT_FLIP_COLOR, DEFAULT_FLIP_STRENGTH, DEFAULT_MASK_COLORS,
};

#[cfg(test)]
mod tests;
