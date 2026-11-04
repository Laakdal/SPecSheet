//! The interior's exterior-view model at the cabin's `ext_interior`
//! locator (finding F10), and its full cockpit at the same locator for the
//! interior camera. The cockpit's locators are relative to the driver's
//! eye (the steering wheel at (0, −0.37, −0.55), the cab centre at
//! x +0.683, and `ext_interior` at x −0.683 in truck space), so the
//! locator is the eye point (Hypothesis).

use specsheet_formats::pmg::Locator;
use specsheet_truck::AccessoryKind;

use specsheet_data::model::ModelCache;
use specsheet_data::GameFs;

use super::{Drive, Placement};

/// Cabin locator the interior's exterior-view model attaches to.
const INTERIOR_LOCATOR: &str = "ext_interior";

/// Places the selected interior's exterior-view model at the cabin's
/// `ext_interior` locator (all 26 presets fit there; six would not fit at
/// the origin, interior survey E3).
/// Returns the exterior-view placements and the cockpit, if any.
/// The visible locators of the interiors' exterior models, in each model's
/// own coordinates: the game's pin builder searches them before the
/// cabin's, without their `ext_interior` placement (F5,
/// `research/evidence/rea-pin-model-order.txt`). In 1.60 only the Renault T
/// 2024 interiors carry exterior slots there (roof light, flags, plates).
pub(super) fn exterior_model_locators(
    fs: &GameFs,
    cache: &ModelCache,
    placements: &[Placement],
) -> Vec<Locator> {
    placements
        .iter()
        .filter_map(|p| {
            let model = cache.load(fs, &p.model).ok()?;
            let variant = model.variant_index(p.variant.as_deref());
            Some(model.visible_locators(variant).cloned().collect::<Vec<_>>())
        })
        .flatten()
        .collect()
}

pub(super) fn interior_placements(
    selected: &[&specsheet_truck::Accessory],
    base_locators: &[Locator],
    drive: Drive,
    warnings: &mut Vec<String>,
) -> (Vec<Placement>, Option<Placement>, Option<Placement>) {
    let mut placements = Vec::new();
    let mut cockpit = None;
    let mut animated = None;
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
        let transform = super::transform::from_locator(locator);
        placements.push(Placement {
            slot: accessory.slot.clone(),
            accessory: accessory.id.clone(),
            model: model.clone(),
            look: accessory.model.look.clone(),
            variant: variant.cloned(),
            transform,
        });
        if let Some(full) = &accessory.model.cockpit {
            let in_cockpit = |model: &String| Placement {
                slot: accessory.slot.clone(),
                accessory: accessory.id.clone(),
                model: model.clone(),
                look: accessory.model.cockpit_look.clone(),
                variant: accessory.model.cockpit_variant.clone(),
                transform,
            };
            cockpit = Some(in_cockpit(full));
            // The moving parts sit in the cockpit's space, at rest (their
            // animations are not played).
            animated = accessory.model.cockpit_animated.as_ref().map(in_cockpit);
        }
    }
    (placements, cockpit, animated)
}
