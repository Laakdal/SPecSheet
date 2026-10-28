//! The design tokens of `specsheet-theme` (D16) mapped onto egui, for the
//! app's own UI only; the game's tuning screen keeps the game's colours.

use bevy_egui::egui::Color32;
use specsheet_theme::{Palette, Rgb, DARK};

/// The appearance the app draws with.
pub const PALETTE: Palette = DARK;

pub const fn color(Rgb(r, g, b): Rgb) -> Color32 {
    Color32::from_rgb(r, g, b)
}
