//! Foundation types shared by every crate (D17): string ids of trucks,
//! slots and accessories; small 3D maths ([`Aabb`], [`Transform`]); the
//! sRGB transfer function. No game formats, no domain rules, no I/O.

pub mod colour;
mod ids;
pub mod math;

pub use ids::{AccessoryId, SlotId, TruckId};
pub use math::{Aabb, Transform};
