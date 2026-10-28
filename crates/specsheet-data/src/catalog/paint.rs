//! Paint jobs: their attributes, the per-part files and the six colours of a
//! saved or preset paint (F15).

use std::collections::HashMap;

use specsheet_core::{AccessoryId, SlotId, TruckId};
use specsheet_formats::sii::{Unit, Value};
use specsheet_truck::{Accessory, Flake, Flip, PaintJob, PaintLocks, PaintOverride, PartPaint};

use super::owned;

/// Per-part paint files: `paint_job/accessory/<paint job token>.sii` (F15).
pub(super) const PAINT_PARTS_DIR: &str = "paint_job/accessory";

/// One `simple_paint_job_data` unit before its `acc_list[]` is resolved.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct RawPart {
    pub(super) acc_list: Vec<String>,
    pub(super) mask: Option<String>,
    pub(super) flake_uvscale: Option<f32>,
    pub(super) flake_vratio: Option<f32>,
}

pub(super) fn raw_part(unit: &Unit) -> Option<RawPart> {
    (unit.class == "simple_paint_job_data").then(|| RawPart {
        acc_list: owned(unit.array_str("acc_list")),
        mask: unit.get_str("paint_job_mask").map(str::to_owned),
        flake_uvscale: unit.get_f32("flake_uvscale"),
        flake_vratio: unit.get_f32("flake_vratio"),
    })
}

/// Gives every paint job the per-part units of its file. The file is named
/// after the first token of the paint job's unit name, not after its own
/// definition file (`color0m.sii` holds `col0m_s_s_a.<truck>.paint_job`,
/// whose parts are in `accessory/col0m_s_s_a.sii`; F15). An `acc_list[]`
/// entry `<slot>.<accessory>` names the unit `<accessory>.<truck>.<slot>`.
pub(super) fn attach_paint_parts(
    accessories: &mut [Accessory],
    truck: &TruckId,
    parts: &HashMap<String, Vec<RawPart>>,
) {
    if parts.is_empty() {
        return;
    }
    let slots: HashMap<AccessoryId, SlotId> = accessories
        .iter()
        .map(|a| (a.id.clone(), a.slot.clone()))
        .collect();
    let resolve = |entry: &str| -> Option<(SlotId, AccessoryId)> {
        let (slot, item) = entry.split_once('.')?;
        let id = AccessoryId::new(format!("{item}.{truck}.{slot}"));
        let slot = slots.get(&id)?.clone();
        Some((slot, id))
    };
    for accessory in accessories.iter_mut() {
        let token = accessory
            .id
            .as_str()
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let (Some(paint), Some(raw)) = (accessory.paint.as_mut(), parts.get(&token)) else {
            continue;
        };
        paint.parts = raw
            .iter()
            .map(|raw| {
                let mut part = PartPaint {
                    mask: raw.mask.clone(),
                    flake_uvscale: raw.flake_uvscale,
                    flake_vratio: raw.flake_vratio,
                    ..PartPaint::default()
                };
                for entry in &raw.acc_list {
                    match resolve(entry) {
                        Some(found) => part.accessories.push(found),
                        None => part.unresolved.push(entry.clone()),
                    }
                }
                part
            })
            .collect();
    }
}

/// The attributes of an `accessory_paint_job_data` unit (F15).
pub(super) fn paint_job_from_unit(unit: &Unit) -> PaintJob {
    let flag = |key: &str| unit.get_bool(key).unwrap_or(false);
    let color = |key: &str| unit.get(key).and_then(color3);
    let color_variants = unit
        .array("color_variant")
        .chunks_exact(4)
        .filter_map(|group| match group {
            [a, b, c, d] => Some([color3(a)?, color3(b)?, color3(c)?, color3(d)?]),
            _ => None,
        })
        .collect();
    PaintJob {
        base_color: color("base_color"),
        mask: unit.get_str("paint_job_mask").map(str::to_owned),
        mask_colors: [
            color("mask_r_color"),
            color("mask_g_color"),
            color("mask_b_color"),
        ],
        airbrush: flag("airbrush"),
        alternate_uvset: flag("alternate_uvset"),
        flipflake: flag("flipflake"),
        flake: Flake {
            color: color("flake_color"),
            density: unit.get_f32("flake_density"),
            shininess: unit.get_f32("flake_shininess"),
            clearcoat_rolloff: unit.get_f32("flake_clearcoat_rolloff"),
            uvscale: unit.get_f32("flake_uvscale"),
            noise: unit.get_str("flake_noise").map(str::to_owned),
        },
        flip: Flip {
            color: color("flip_color"),
            strength: unit.get_f32("flip_strength"),
        },
        locks: PaintLocks {
            base: flag("base_color_locked"),
            mask: [
                flag("mask_r_locked"),
                flag("mask_g_locked"),
                flag("mask_b_locked"),
            ],
            flake: flag("flake_color_locked"),
            flip: flag("flip_color_locked"),
        },
        stock: flag("stock"),
        color_variants,
        parts: Vec::new(),
    }
}

/// An RGB tuple; extra components are ignored, fewer than three fail.
pub(super) fn color3(value: &Value) -> Option<[f32; 3]> {
    let floats = value.as_floats()?;
    <[f32; 3]>::try_from(floats.get(..3)?).ok()
}

/// The six colours of a preset's or save's `vehicle_paint_job_accessory`
/// (F15). A key that is present but not a colour is reported.
pub(super) fn paint_override(unit: &Unit, mut warn: impl FnMut(String)) -> PaintOverride {
    let mut color = |key: &str| {
        let value = unit.get(key)?;
        let parsed = color3(value);
        if parsed.is_none() {
            warn(format!("{}: {key} {value} is not a colour", unit.name));
        }
        parsed
    };
    PaintOverride {
        base: color("base_color"),
        mask: [
            color("mask_r_color"),
            color("mask_g_color"),
            color("mask_b_color"),
        ],
        flake: color("flake_color"),
        flip: color("flip_color"),
    }
}
