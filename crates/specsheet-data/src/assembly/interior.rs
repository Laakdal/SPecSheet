//! The interior's exterior-view model at the cabin's `ext_interior`
//! locator (finding F10).

use specsheet_core::AccessoryKind;
use specsheet_formats::pmg::Locator;

use super::{Drive, Placement, Transform};

/// Cabin locator the interior's exterior-view model attaches to.
const INTERIOR_LOCATOR: &str = "ext_interior";

/// Places the selected interior's exterior-view model at the cabin's
/// `ext_interior` locator (all 26 presets fit there; six would not fit at
/// the origin, interior survey E3).
pub(super) fn interior_placements(
    selected: &[&specsheet_core::Accessory],
    base_locators: &[Locator],
    drive: Drive,
    warnings: &mut Vec<String>,
) -> Vec<Placement> {
    let mut placements = Vec::new();
    for accessory in selected
        .iter()
        .filter(|a| a.kind == AccessoryKind::Interior)
    {
        // Right-hand drive uses the interior's UK twin (finding F10).
        let (model, variant) = match (drive, accessory.model.exterior_uk.as_ref()) {
            (Drive::Right, Some(uk)) => (Some(uk), accessory.model.variant_uk.as_ref()),
            _ => (
                accessory.model.exterior.as_ref(),
                accessory.model.variant.as_ref(),
            ),
        };
        let Some(model) = model else {
            continue;
        };
        let Some(locator) = base_locators.iter().find(|l| l.name == INTERIOR_LOCATOR) else {
            warnings.push(format!(
                "{}: cabin has no visible `{INTERIOR_LOCATOR}` locator",
                accessory.id
            ));
            continue;
        };
        placements.push(Placement {
            slot: accessory.slot.clone(),
            accessory: accessory.id.clone(),
            model: model.clone(),
            look: accessory.model.look.clone(),
            variant: variant.cloned(),
            transform: Transform::from_locator(locator),
        });
    }
    placements
}
