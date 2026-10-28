//! Wheel parts at the chassis wheel locators (finding F9).

use specsheet_formats::pmg::Locator;
use specsheet_truck::{AccessoryId, AccessoryKind, WheelPart};

use super::{Placement, Transform};

/// Places the selected wheel parts at the front (`f_*` slots) and rear
/// wheel locators, using model variant `left` for x < 0 and `right`
/// otherwise.
///
/// Tyres are moved along the wheel axis by the `tire_shift` of the axle's
/// rim/disc (finding F9): towards the truck's centre line when positive.
/// Hub-face parts stay at the locator. Locator rotations are ignored.
pub(super) fn wheel_placements(
    selected: &[&specsheet_truck::Accessory],
    front: &[&Locator],
    rear: &[&Locator],
    warnings: &mut Vec<String>,
) -> Vec<Placement> {
    let is_front = |a: &specsheet_truck::Accessory| a.slot.as_str().starts_with("f_");
    let wheel_parts: Vec<_> = selected
        .iter()
        .filter(|a| a.kind == AccessoryKind::Wheel)
        .collect();
    let mut placements = Vec::new();
    for (front_axle, locators) in [(true, front), (false, rear)] {
        let axle: Vec<_> = wheel_parts
            .iter()
            .filter(|a| is_front(a) == front_axle)
            .collect();
        let shifts: Vec<(&AccessoryId, f32)> = axle
            .iter()
            .filter_map(|a| match a.wheel {
                Some(WheelPart::Rim {
                    tire_shift: Some(shift),
                }) => Some((&a.id, shift)),
                _ => None,
            })
            .collect();
        if shifts.len() > 1 {
            warnings.push(format!(
                "several tire_shift values on one axle, using the first: {shifts:?}"
            ));
        }
        let tire_shift = shifts.first().map_or(0.0, |(_, shift)| *shift);
        for accessory in axle {
            let Some(model) = accessory.model.exterior.as_ref() else {
                continue;
            };
            let shift = if accessory.wheel == Some(WheelPart::Tire) {
                tire_shift
            } else {
                0.0
            };
            for locator in locators {
                let left = locator.position[0] < 0.0;
                // Wheels take only the locator's position (finding F9): a
                // few chassis carry yawed wheel locators.
                let mut transform = Transform {
                    translation: locator.position,
                    scale: locator.scale,
                    ..Transform::IDENTITY
                };
                // The model's +x points inboard on the left, outboard on
                // the right.
                transform.translation[0] += if left { shift } else { -shift } * locator.scale;
                placements.push(Placement {
                    slot: accessory.slot.clone(),
                    accessory: accessory.id.clone(),
                    model: model.clone(),
                    look: accessory.model.look.clone(),
                    variant: Some(if left { "left" } else { "right" }.to_owned()),
                    transform,
                });
            }
        }
    }
    placements
}
