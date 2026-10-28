//! Placement transforms from model locators.

use specsheet_core::Transform;
use specsheet_formats::pmg::Locator;

/// The transform of a model placed at `locator`. Locators store their
/// rotation `(w, x, y, z)`; [`Transform`] uses `(x, y, z, w)`.
pub(super) fn from_locator(locator: &Locator) -> Transform {
    let [w, x, y, z] = locator.rotation_wxyz;
    Transform {
        translation: locator.position,
        rotation: [x, y, z, w],
        scale: locator.scale,
    }
}
