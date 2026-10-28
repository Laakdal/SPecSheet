//! The paint colour overrides of a configuration (F15).

use serde::{Deserialize, Serialize};

use super::Configuration;
use crate::catalog::{Accessory, AccessoryKind, TruckCatalog};

/// Colour overrides a configuration carries for its paint job: the six
/// colours presets and saves store in `vehicle_paint_job_accessory` (F15).
/// `None` uses the paint job's own value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PaintOverride {
    /// `base_color`.
    #[serde(default)]
    pub base: Option<[f32; 3]>,
    /// `mask_r_color`, `mask_g_color`, `mask_b_color`.
    #[serde(default)]
    pub mask: [Option<[f32; 3]>; 3],
    /// `flake_color`.
    #[serde(default)]
    pub flake: Option<[f32; 3]>,
    /// `flip_color`.
    #[serde(default)]
    pub flip: Option<[f32; 3]>,
}

impl PaintOverride {
    /// No colour is overridden.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

impl Configuration {
    /// Sets the base colour override (`None` uses the paint job's).
    pub fn set_paint_color(&mut self, color: Option<[f32; 3]>) {
        self.paint.base = color;
    }

    /// The effective body colour: the override, else the selected paint
    /// job's `base_color`.
    pub fn paint_color(&self, catalog: &TruckCatalog) -> Option<[f32; 3]> {
        self.paint.base.or_else(|| {
            self.selections
                .values()
                .filter_map(|id| catalog.accessory(id))
                .find_map(|a| a.base_color)
        })
    }

    /// Every colour override of the paint job.
    pub fn paint_override(&self) -> &PaintOverride {
        &self.paint
    }

    /// Replaces every colour override at once (e.g. from a preset).
    pub fn set_paint_override(&mut self, paint: PaintOverride) {
        self.paint = paint;
    }

    /// Sets the override of mask colour `channel` (0 red, 1 green, 2 blue).
    /// Other channels are ignored.
    pub fn set_mask_color(&mut self, channel: usize, color: Option<[f32; 3]>) {
        if let Some(slot) = self.paint.mask.get_mut(channel) {
            *slot = color;
        }
    }

    /// Sets the flake colour override.
    pub fn set_flake_color(&mut self, color: Option<[f32; 3]>) {
        self.paint.flake = color;
    }

    /// Sets the flip colour override.
    pub fn set_flip_color(&mut self, color: Option<[f32; 3]>) {
        self.paint.flip = color;
    }

    /// The selected paint job, if any.
    pub fn paint_job<'a>(&self, catalog: &'a TruckCatalog) -> Option<&'a Accessory> {
        self.selections
            .values()
            .filter_map(|id| catalog.accessory(id))
            .find(|a| a.kind == AccessoryKind::PaintJob)
    }
}
