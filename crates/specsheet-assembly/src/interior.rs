//! The interior's exterior-view model at the cabin's `ext_interior`
//! locator (finding F10), and its full cockpit at the driver's eye point for
//! the interior camera. The eye point is the first visible locator whose
//! name starts with `interior` (`interior`/`interior_uk`, `interior_s`/
//! `interior_s1`, `interior1`, …): the cockpit's locators are relative to
//! it (the steering wheel at x ≈ 0, 0.3–0.5 m below), and the game places
//! the cockpit there, position only (`research/evidence/rea-pin-roots.txt`,
//! `research/evidence/interior-locators.md`).

use specsheet_core::Transform;
use specsheet_formats::pmg::Locator;
use specsheet_truck::AccessoryKind;

use specsheet_data::model::ModelCache;
use specsheet_data::GameFs;

use super::{Drive, Placement};

/// Cabin locator the interior's exterior-view model attaches to.
const INTERIOR_LOCATOR: &str = "ext_interior";

/// Prefix of the locator names that hold the driver's eye point.
const EYE_LOCATOR: &str = "interior";

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

/// Places the selected interior's exterior-view model at the cabin's
/// `ext_interior` locator (all 26 presets fit there; six would not fit at
/// the origin, interior survey E3).
pub(super) fn interior_placements(
    selected: &[&specsheet_truck::Accessory],
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
            transform: super::transform::from_locator(locator),
        });
    }
    placements
}

/// The driver's eye point: the position of the first of `locators` whose
/// name starts with `interior`, unrotated. `locators` are in the game's
/// search order (the interior's exterior model, the cabin, the chassis)
/// and hold the visible parts only, so the cabin's variant picks the drive
/// side (`interior` on the EU variant, `interior_uk` on the UK one).
fn eye_point(locators: &[Locator]) -> Option<Transform> {
    locators
        .iter()
        .find(|l| l.name.starts_with(EYE_LOCATOR))
        .map(|l| Transform {
            translation: l.position,
            ..Transform::IDENTITY
        })
}

/// The selected interior's full cockpit and its moving parts, at the
/// driver's eye point found in `locators` (see [`eye_point`]).
pub(super) fn cockpit_placements(
    selected: &[&specsheet_truck::Accessory],
    locators: &[Locator],
    warnings: &mut Vec<String>,
) -> (Option<Placement>, Option<Placement>) {
    let Some(accessory) = selected.iter().find(|a| a.kind == AccessoryKind::Interior) else {
        return (None, None);
    };
    let Some(full) = &accessory.model.cockpit else {
        return (None, None);
    };
    let Some(transform) = eye_point(locators) else {
        warnings.push(format!(
            "{}: cabin has no visible `{EYE_LOCATOR}*` locator",
            accessory.id
        ));
        return (None, None);
    };
    let in_cockpit = |model: &String| Placement {
        slot: accessory.slot.clone(),
        accessory: accessory.id.clone(),
        model: model.clone(),
        look: accessory.model.cockpit_look.clone(),
        variant: accessory.model.cockpit_variant.clone(),
        transform,
    };
    // The moving parts sit in the cockpit's space, at rest (their
    // animations are not played).
    let animated = accessory.model.cockpit_animated.as_ref().map(in_cockpit);
    (Some(in_cockpit(full)), animated)
}
