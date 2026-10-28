//! Game materials drawn with our own shaders, written clean-room from the
//! specs in `docs/app/shader-specs/` (D13). This tree depends only on Bevy,
//! `specsheet-data` and the upload checks in `assets`, so it can move into
//! the planned `specsheet-render` crate (D17) as it is.

mod lighting;
mod truckpaint;

pub use lighting::{GameLight, GameLighting, GameLightingPlugin};
pub use truckpaint::{Paint, PaintMaterials, PaintSurface, TruckPaintMaterial, TruckPaintPlugin};
