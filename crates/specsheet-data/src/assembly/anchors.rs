//! Hotspot anchors: where each slot's marker sits (design D7).

use std::collections::BTreeMap;

use specsheet_core::SlotId;
use specsheet_formats::pmg::{Aabb, Locator};

use super::transform::transform_aabb;
use super::Placement;
use crate::fs::GameFs;
use crate::model::ModelCache;

/// Where a slot's clickable marker sits, in truck space.
#[derive(Clone, Debug, PartialEq)]
pub struct Anchor {
    pub slot: SlotId,
    pub position: [f32; 3],
    /// The slot's parts sit inside the cabin (interior accessories).
    pub interior: bool,
    /// Nothing is placed in the slot; the anchor sits at its bare locators,
    /// which can lie away from any visible geometry (e.g. below a bumper).
    pub empty: bool,
}

/// World-space bounding box of every slot's placed models.
pub(super) fn placed_boxes(
    fs: &GameFs,
    cache: &ModelCache,
    placements: &[Placement],
) -> BTreeMap<SlotId, Aabb> {
    let mut boxes: BTreeMap<SlotId, Aabb> = BTreeMap::new();
    for placement in placements {
        let Ok(model) = cache.load(fs, &placement.model) else {
            continue;
        };
        let world = transform_aabb(&model.pmg.aabb, &placement.transform);
        boxes
            .entry(placement.slot.clone())
            .and_modify(|b| *b = b.union(&world))
            .or_insert(world);
    }
    boxes
}

/// Margin by which a model must stick out of the cabin box to count as
/// exterior; absorbs parts modelled flush with the cabin surface.
const INTERIOR_MARGIN: f32 = 0.05;

/// One anchor per slot (design choice D7 in `docs/DECISIONS.md`):
/// * addon slots anchor at their placed models' bounding-box centre, or at
///   their locators while empty. A centre hidden inside the cabin (models
///   spanning both sides, e.g. mirror pairs) moves to the box's left (-x)
///   face so the marker stays on visible geometry;
/// * a slot is "interior" when its models (or locators) lie inside the
///   cabin box;
/// * an anchor is "empty" when no model is placed in its slot;
/// * base slots anchor at their model centre.
///
/// Wheel parts have no anchor: their pins come from [`crate::pins`].
pub(super) fn anchors(
    by_slot: &BTreeMap<SlotId, Vec<&Locator>>,
    slot_boxes: &BTreeMap<SlotId, Aabb>,
    base_boxes: &[(Aabb, &specsheet_core::Accessory)],
    cabin_box: Option<Aabb>,
) -> Vec<Anchor> {
    let cabin_inner = cabin_box.map(|cabin| cabin.shrunk(INTERIOR_MARGIN));
    let inside = |p: [f32; 3]| cabin_inner.is_some_and(|c| c.contains_point(p));
    let mut anchors: Vec<Anchor> = by_slot
        .iter()
        .map(|(slot, locators)| match slot_boxes.get(slot) {
            Some(aabb) => {
                let center = aabb.center();
                let interior = cabin_inner.is_some_and(|c| c.contains_box(aabb));
                let position = if inside(center) && !interior {
                    [aabb.min[0], center[1], center[2]]
                } else {
                    center
                };
                Anchor {
                    slot: slot.clone(),
                    position,
                    interior,
                    empty: false,
                }
            }
            None => {
                let position = mean(locators.iter().map(|l| l.position));
                Anchor {
                    slot: slot.clone(),
                    position,
                    interior: inside(position),
                    empty: true,
                }
            }
        })
        .collect();
    for (aabb, accessory) in base_boxes {
        anchors.push(Anchor {
            slot: accessory.slot.clone(),
            position: aabb.center(),
            interior: false,
            empty: false,
        });
    }
    anchors
}

fn mean(points: impl Iterator<Item = [f32; 3]>) -> [f32; 3] {
    let (sum, count) = points.fold(([0.0f32; 3], 0usize), |(s, n), p| {
        ([s[0] + p[0], s[1] + p[1], s[2] + p[2]], n + 1)
    });
    if count == 0 {
        return [0.0; 3];
    }
    let n = count as f32;
    [sum[0] / n, sum[1] / n, sum[2] / n]
}
