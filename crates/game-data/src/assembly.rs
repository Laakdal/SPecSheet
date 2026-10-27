//! Assembles a configured truck into model placements and hotspot anchors.
//!
//! * Chassis and cabin models share one coordinate space and are placed at
//!   the origin (finding F4).
//! * Addons with an exterior model are placed at every visible base-model
//!   locator of their slot ([`crate::slots`]).
//! * Wheel parts are placed at the chassis `wheel_f*` / `wheel_r*` locators;
//!   their models have `left`/`right` variants, chosen by the locator side.
//!   Tyres are offset by the rim's `tire_shift` (finding F9).
//! * Hookups are placed at the `slot_NN` locators of their host accessory's
//!   model, composed with the host's own placement (finding F11).
//! * The selected interior's light exterior-view model (`ext_model`) is
//!   placed at the cabin's `ext_interior` locator, so the cab is not hollow
//!   when seen through the windows (`research/evidence/interior-survey.txt`).

use std::collections::{BTreeMap, BTreeSet};

use scs_formats::pmg::{Aabb, Locator};
use tuning_core::{
    AccessoryId, AccessoryKind, Configuration, SlotAvailability, SlotId, TruckCatalog, WheelPart,
};

use crate::fs::GameFs;
use crate::model::ModelCache;
use crate::slots::locators_by_slot;

/// Rigid transform of a placed model, in game (truck) space: x right,
/// y up, -z forward.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub translation: [f32; 3],
    /// Unit quaternion `(x, y, z, w)`.
    pub rotation: [f32; 4],
    pub scale: f32,
}

impl Transform {
    pub const IDENTITY: Self = Self {
        translation: [0.0; 3],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: 1.0,
    };

    /// `self` applied after `child`: the child's transform expressed in
    /// the parent's space (parent ∘ child).
    pub fn compose(&self, child: &Transform) -> Transform {
        let scaled = child.translation.map(|c| c * self.scale);
        let rotated = rotate(self.rotation, scaled);
        Transform {
            translation: [0, 1, 2].map(|i| self.translation[i] + rotated[i]),
            rotation: quat_mul(self.rotation, child.rotation),
            scale: self.scale * child.scale,
        }
    }

    fn from_locator(locator: &Locator) -> Self {
        let [w, x, y, z] = locator.rotation_wxyz;
        Self {
            translation: locator.position,
            rotation: [x, y, z, w],
            scale: locator.scale,
        }
    }
}

/// One model instance of the assembled truck.
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub slot: SlotId,
    pub accessory: AccessoryId,
    pub model: String,
    pub look: Option<String>,
    pub variant: Option<String>,
    pub transform: Transform,
}

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
    /// Anchors sharing a group (e.g. every front wheel part) have the same
    /// position and form one marker. The key is the locator prefix the
    /// group's parts attach to (`wheel_f`, `wheel_r`).
    pub group: Option<String>,
}

/// A position on a host accessory where a hookup can be mounted.
#[derive(Clone, Debug, PartialEq)]
pub struct HookupPosition {
    /// The `slot_NN` locator name, as stored in configurations.
    pub name: String,
    /// Truck-space position (on the first placement of the host).
    pub position: [f32; 3],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Assembly {
    pub placements: Vec<Placement>,
    /// Hookup positions offered by each placed host accessory, by slot.
    pub hookup_positions: BTreeMap<SlotId, Vec<HookupPosition>>,
    pub availability: SlotAvailability,
    pub anchors: Vec<Anchor>,
    /// Non-fatal problems (missing models, unmatched slots).
    pub warnings: Vec<String>,
}

/// Right-hand-drive (UK) or left-hand-drive assembly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Drive {
    #[default]
    Left,
    Right,
}

const FRONT_WHEELS: &str = "wheel_f";
const REAR_WHEELS: &str = "wheel_r";
/// Cabin locator the interior's exterior-view model attaches to.
const INTERIOR_LOCATOR: &str = "ext_interior";
/// Prefix of the hookup position locators on host accessory models.
const HOOKUP_LOCATOR: &str = "slot_";

/// Assembles `config`. Base models that fail to load are reported as
/// warnings; the rest of the truck is still assembled.
pub fn assemble(
    fs: &GameFs,
    cache: &ModelCache,
    catalog: &TruckCatalog,
    config: &Configuration,
    drive: Drive,
) -> Assembly {
    let mut assembly = Assembly::default();
    let selected: Vec<_> = config
        .selections()
        .values()
        .filter_map(|id| catalog.accessory(id))
        .collect();

    let base = base_layout(fs, cache, catalog, config, drive);
    assembly.warnings = base.warnings;
    assembly.placements = base.placements;
    let base_locators = base.locators;
    let base_boxes = base.boxes;

    let addon_slots = addon_slots(catalog);
    let by_slot = locators_by_slot(&addon_slots, base_locators.iter());
    assembly.availability = SlotAvailability {
        available: by_slot.keys().cloned().collect(),
    };

    for accessory in selected.iter().filter(|a| a.needs_exterior_locator()) {
        let model = match drive {
            Drive::Right => accessory
                .model
                .exterior_uk
                .as_ref()
                .or(accessory.model.exterior.as_ref()),
            Drive::Left => accessory.model.exterior.as_ref(),
        };
        let (Some(model), Some(locators)) = (model, by_slot.get(&accessory.slot)) else {
            assembly.warnings.push(format!(
                "{}: no locator for slot {}",
                accessory.id, accessory.slot
            ));
            continue;
        };
        for locator in locators {
            assembly.placements.push(Placement {
                slot: accessory.slot.clone(),
                accessory: accessory.id.clone(),
                model: model.clone(),
                look: accessory.model.look.clone(),
                variant: accessory.model.variant.clone(),
                transform: Transform::from_locator(locator),
            });
        }
    }

    // Wheels.
    let wheel_locators = |prefix: &str| -> Vec<&Locator> {
        base_locators
            .iter()
            .filter(|l| l.name.starts_with(prefix))
            .collect()
    };
    let front = wheel_locators(FRONT_WHEELS);
    let rear = wheel_locators(REAR_WHEELS);
    let wheels = wheel_placements(&selected, &front, &rear, &mut assembly.warnings);
    assembly.placements.extend(wheels);
    let interior = interior_placements(&selected, &base_locators, &mut assembly.warnings);
    assembly.placements.extend(interior);
    let (positions, hookups) = hookup_placements(fs, cache, catalog, config, &assembly.placements);
    assembly.hookup_positions = positions;
    for (placement, warning) in hookups {
        match placement {
            Some(placement) => assembly.placements.push(placement),
            None => assembly.warnings.extend(warning),
        }
    }

    let cabin_box = base_boxes
        .iter()
        .find(|(_, a)| a.kind == AccessoryKind::Cabin)
        .map(|(aabb, _)| *aabb);
    let slot_boxes = placed_boxes(fs, cache, &assembly.placements);
    assembly.anchors = anchors(
        catalog,
        &by_slot,
        &slot_boxes,
        &base_boxes,
        cabin_box,
        &front,
        &rear,
    );
    assembly
}

/// Places the selected wheel parts at the front (`f_*` slots) and rear
/// wheel locators, using model variant `left` for x < 0 and `right`
/// otherwise.
///
/// Tyres are moved along the wheel axis by the `tire_shift` of the axle's
/// rim/disc (finding F9): towards the truck's centre line when positive.
/// Hub-face parts stay at the locator. Locator rotations are ignored.
fn wheel_placements(
    selected: &[&tuning_core::Accessory],
    front: &[&Locator],
    rear: &[&Locator],
    warnings: &mut Vec<String>,
) -> Vec<Placement> {
    let is_front = |a: &tuning_core::Accessory| a.slot.as_str().starts_with("f_");
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

type HookupResult = (Option<Placement>, Option<String>);

/// The hookup positions of every placed host accessory, and the placements
/// of the configured hookups (or a warning per hookup that cannot be
/// placed). A hookup's transform is its host's placement composed with the
/// position locator.
fn hookup_placements(
    fs: &GameFs,
    cache: &ModelCache,
    catalog: &TruckCatalog,
    config: &Configuration,
    placements: &[Placement],
) -> (BTreeMap<SlotId, Vec<HookupPosition>>, Vec<HookupResult>) {
    let mut positions = BTreeMap::new();
    let mut hosts: BTreeMap<&SlotId, Vec<(&Placement, Vec<Locator>)>> = BTreeMap::new();
    for placement in placements {
        let Ok(model) = cache.load(fs, &placement.model) else {
            continue;
        };
        let variant = model.variant_index(placement.variant.as_deref());
        let locators: Vec<Locator> = model
            .visible_locators(variant)
            .filter(|l| l.name.starts_with(HOOKUP_LOCATOR))
            .cloned()
            .collect();
        if locators.is_empty() {
            continue;
        }
        positions.entry(placement.slot.clone()).or_insert_with(|| {
            locators
                .iter()
                .map(|l| HookupPosition {
                    name: l.name.clone(),
                    position: placement
                        .transform
                        .compose(&Transform::from_locator(l))
                        .translation,
                })
                .collect()
        });
        hosts
            .entry(&placement.slot)
            .or_default()
            .push((placement, locators));
    }
    let mut results = Vec::new();
    for (host, position, id) in config.all_hookups() {
        let Some(model) = catalog.hookup(id).and_then(|h| h.model.as_ref()) else {
            results.push((None, Some(format!("hookup {id} has no model"))));
            continue;
        };
        let mut placed = false;
        for (host_placement, locators) in hosts.get(host).into_iter().flatten() {
            if let Some(locator) = locators.iter().find(|l| l.name == position) {
                placed = true;
                results.push((
                    Some(Placement {
                        slot: host.clone(),
                        accessory: id.clone(),
                        model: model.clone(),
                        look: None,
                        variant: None,
                        transform: host_placement
                            .transform
                            .compose(&Transform::from_locator(locator)),
                    }),
                    None,
                ));
            }
        }
        if !placed {
            results.push((
                None,
                Some(format!("hookup {id}: no position `{position}` on {host}")),
            ));
        }
    }
    (positions, results)
}

/// Places the selected interior's exterior-view model at the cabin's
/// `ext_interior` locator (all 26 presets fit there; six would not fit at
/// the origin, interior survey E3).
fn interior_placements(
    selected: &[&tuning_core::Accessory],
    base_locators: &[Locator],
    warnings: &mut Vec<String>,
) -> Vec<Placement> {
    let mut placements = Vec::new();
    for accessory in selected
        .iter()
        .filter(|a| a.kind == AccessoryKind::Interior)
    {
        let Some(model) = accessory.model.exterior.as_ref() else {
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
            variant: accessory.model.variant.clone(),
            transform: Transform::from_locator(locator),
        });
    }
    placements
}

/// World-space bounding box of every slot's placed models.
fn placed_boxes(
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
/// * wheel slots anchor at the left wheels of their axle group and share
///   the group (`wheel_f` / `wheel_r`); base slots anchor at their model
///   centre.
fn anchors(
    catalog: &TruckCatalog,
    by_slot: &BTreeMap<SlotId, Vec<&Locator>>,
    slot_boxes: &BTreeMap<SlotId, Aabb>,
    base_boxes: &[(Aabb, &tuning_core::Accessory)],
    cabin_box: Option<Aabb>,
    front: &[&Locator],
    rear: &[&Locator],
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
                    group: None,
                }
            }
            None => {
                let position = mean(locators.iter().map(|l| l.position));
                Anchor {
                    slot: slot.clone(),
                    position,
                    interior: inside(position),
                    empty: true,
                    group: None,
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
            group: None,
        });
    }
    let mut wheel_slots: Vec<SlotId> = catalog
        .accessories()
        .filter(|a| a.kind == AccessoryKind::Wheel)
        .map(|a| a.slot.clone())
        .collect();
    wheel_slots.sort();
    wheel_slots.dedup();
    for slot in wheel_slots {
        let (group, locators) = if slot.as_str().starts_with("f_") {
            (FRONT_WHEELS, front)
        } else {
            (REAR_WHEELS, rear)
        };
        let left: Vec<[f32; 3]> = locators
            .iter()
            .map(|l| l.position)
            .filter(|p| p[0] < 0.0)
            .collect();
        if !left.is_empty() {
            anchors.push(Anchor {
                empty: !slot_boxes.contains_key(&slot),
                slot,
                position: mean(left.into_iter()),
                interior: false,
                group: Some(group.to_owned()),
            });
        }
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

/// Base models (chassis, cabin) of a configuration: their placements,
/// visible locators and bounding boxes.
struct BaseLayout<'a> {
    placements: Vec<Placement>,
    locators: Vec<Locator>,
    boxes: Vec<(Aabb, &'a tuning_core::Accessory)>,
    warnings: Vec<String>,
}

fn base_layout<'a>(
    fs: &GameFs,
    cache: &ModelCache,
    catalog: &'a TruckCatalog,
    config: &Configuration,
    drive: Drive,
) -> BaseLayout<'a> {
    let mut layout = BaseLayout {
        placements: Vec::new(),
        locators: Vec::new(),
        boxes: Vec::new(),
        warnings: Vec::new(),
    };
    let bases = config
        .selections()
        .values()
        .filter_map(|id| catalog.accessory(id))
        .filter(|a| a.kind.is_base());
    for accessory in bases {
        let Some(path) = accessory.model.exterior.as_deref() else {
            continue;
        };
        let variant = match drive {
            Drive::Right => accessory
                .model
                .variant_uk
                .as_ref()
                .or(accessory.model.variant.as_ref()),
            Drive::Left => accessory.model.variant.as_ref(),
        };
        match cache.load(fs, path) {
            Ok(model) => {
                let index = model.variant_index(variant.map(String::as_str));
                layout
                    .locators
                    .extend(model.visible_locators(index).cloned());
                layout.placements.push(Placement {
                    slot: accessory.slot.clone(),
                    accessory: accessory.id.clone(),
                    model: path.to_owned(),
                    look: accessory.model.look.clone(),
                    variant: variant.cloned(),
                    transform: Transform::IDENTITY,
                });
                layout.boxes.push((model.pmg.aabb, accessory));
            }
            Err(error) => layout.warnings.push(format!("{}: {error}", accessory.id)),
        }
    }
    layout
}

fn addon_slots(catalog: &TruckCatalog) -> BTreeSet<SlotId> {
    catalog
        .accessories()
        .filter(|a| a.needs_exterior_locator())
        .map(|a| a.slot.clone())
        .collect()
}

/// Which addon slots the base models of `config` can host. Cheaper than a
/// full [`assemble`]; used while the rules repair a configuration.
pub fn slot_availability(
    fs: &GameFs,
    cache: &ModelCache,
    catalog: &TruckCatalog,
    config: &Configuration,
    drive: Drive,
) -> SlotAvailability {
    let base = base_layout(fs, cache, catalog, config, drive);
    SlotAvailability {
        available: locators_by_slot(&addon_slots(catalog), base.locators.iter())
            .into_keys()
            .collect(),
    }
}

/// Hamilton product `a * b` of quaternions stored `(x, y, z, w)`.
fn quat_mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

/// Rotates `v` by the unit quaternion `q = (x, y, z, w)`.
fn rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let [x, y, z, w] = q;
    // v' = v + w*t + q_xyz × t, with t = 2 * (q_xyz × v)
    let t = [
        2.0 * (y * v[2] - z * v[1]),
        2.0 * (z * v[0] - x * v[2]),
        2.0 * (x * v[1] - y * v[0]),
    ];
    [
        v[0] + w * t[0] + (y * t[2] - z * t[1]),
        v[1] + w * t[1] + (z * t[0] - x * t[2]),
        v[2] + w * t[2] + (x * t[1] - y * t[0]),
    ]
}

fn transform_aabb(aabb: &Aabb, transform: &Transform) -> Aabb {
    let corners = (0..8).map(|corner| {
        let local = [
            if corner & 1 == 0 {
                aabb.min[0]
            } else {
                aabb.max[0]
            },
            if corner & 2 == 0 {
                aabb.min[1]
            } else {
                aabb.max[1]
            },
            if corner & 4 == 0 {
                aabb.min[2]
            } else {
                aabb.max[2]
            },
        ];
        let rotated = rotate(transform.rotation, local.map(|c| c * transform.scale));
        let world = [0, 1, 2].map(|i| rotated[i] + transform.translation[i]);
        Aabb {
            min: world,
            max: world,
        }
    });
    corners.reduce(|a, b| a.union(&b)).unwrap_or(*aabb)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wheel_part(id: &str, slot: &str, wheel: WheelPart) -> tuning_core::Accessory {
        tuning_core::Accessory {
            id: AccessoryId::from(id),
            slot: SlotId::from(slot),
            kind: AccessoryKind::Wheel,
            def_path: format!("/def/vehicle/{slot}/{id}.sii"),
            name: id.to_owned(),
            icon: None,
            price: None,
            unlock_level: None,
            model: tuning_core::ModelRef {
                exterior: Some(format!("/vehicle/{id}.pmd")),
                ..tuning_core::ModelRef::default()
            },
            suitable_for: Vec::new(),
            conflict_with: Vec::new(),
            defaults: Vec::new(),
            requires: Vec::new(),
            base_color: None,
            wheel: Some(wheel),
        }
    }

    fn locator(name: &str, position: [f32; 3]) -> Locator {
        Locator {
            name: name.to_owned(),
            position,
            scale: 1.0,
            rotation_wxyz: [1.0, 0.0, 0.0, 0.0],
            hookup: None,
        }
    }

    /// Finding F9: the tyre moves by the disc's `tire_shift` towards the
    /// truck's centre line (+x on the left, -x on the right); the disc and
    /// the other hub-face parts stay at the locator.
    #[test]
    fn tyres_move_by_the_rim_tire_shift() {
        let disc = wheel_part(
            "5.fdisc",
            "f_disc",
            WheelPart::Rim {
                tire_shift: Some(-0.038),
            },
        );
        let hub = wheel_part("1.fhub", "f_hub", WheelPart::Rim { tire_shift: None });
        let tyre = wheel_part("1.ftire", "f_tire", WheelPart::Tire);
        let rear_disc = wheel_part(
            "1.rdisc",
            "r_disc",
            WheelPart::Rim {
                tire_shift: Some(-0.01),
            },
        );
        let rear_tyre = wheel_part("1.rtire", "r_tire", WheelPart::Tire);
        let left = locator("wheel_f_0", [-1.0, 0.5, -1.6]);
        let right = locator("wheel_f_1", [1.0, 0.5, -1.6]);
        let rear_left = locator("wheel_r_0", [-0.9, 0.5, 2.0]);
        let mut warnings = Vec::new();
        let placements = wheel_placements(
            &[&disc, &hub, &tyre, &rear_disc, &rear_tyre],
            &[&left, &right],
            &[&rear_left],
            &mut warnings,
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        let x = |id: &str, variant: &str| -> f32 {
            placements
                .iter()
                .find(|p| p.accessory.as_str() == id && p.variant.as_deref() == Some(variant))
                .map(|p| p.transform.translation[0])
                .unwrap_or(f32::NAN)
        };
        let close = |a: f32, b: f32| (a - b).abs() < 1e-6;
        assert!(
            close(x("1.ftire", "left"), -1.038),
            "{}",
            x("1.ftire", "left")
        );
        assert!(
            close(x("1.ftire", "right"), 1.038),
            "{}",
            x("1.ftire", "right")
        );
        assert!(close(x("5.fdisc", "left"), -1.0));
        assert!(close(x("1.fhub", "right"), 1.0));
        assert!(
            close(x("1.rtire", "left"), -0.91),
            "{}",
            x("1.rtire", "left")
        );
        assert!(close(x("1.rdisc", "left"), -0.9));
    }

    /// 11 of 562 chassis wheel locators carry a 90 or 180 degree yaw
    /// (MAN TGX, Renault Magnum, Iveco Stralis); wheels keep the truck's
    /// orientation and only take the locator's position.
    #[test]
    fn wheels_ignore_locator_rotation() {
        let tyre = wheel_part("1.rtire", "r_tire", WheelPart::Tire);
        let disc = wheel_part(
            "1.rdisc",
            "r_disc",
            WheelPart::Rim {
                tire_shift: Some(-0.01),
            },
        );
        let mut yawed = locator("wheel_r_1", [0.93, 0.5, 2.0]);
        yawed.rotation_wxyz = [0.0, 0.0, 1.0, 0.0];
        let mut warnings = Vec::new();
        let placements = wheel_placements(&[&tyre, &disc], &[], &[&yawed], &mut warnings);
        for placement in &placements {
            assert_eq!(placement.transform.rotation, [0.0, 0.0, 0.0, 1.0]);
            assert_eq!(placement.variant.as_deref(), Some("right"));
        }
        let tyre_x = placements
            .iter()
            .find(|p| p.accessory.as_str() == "1.rtire")
            .map(|p| p.transform.translation[0]);
        assert!(
            tyre_x.is_some_and(|x| (x - 0.94).abs() < 1e-6),
            "{tyre_x:?}"
        );
    }

    #[test]
    fn interior_exterior_model_sits_at_the_ext_interior_locator() {
        let mut interior = wheel_part("std.t.interior", "interior", WheelPart::Tire);
        interior.kind = AccessoryKind::Interior;
        interior.wheel = None;
        interior.model.variant = Some("s_h_std_eu".to_owned());
        let locators = [
            locator("interior_s", [-0.7, 2.7, -1.9]),
            locator("ext_interior", [-0.683, 2.705, -1.862]),
        ];
        let mut warnings = Vec::new();
        let placements = interior_placements(&[&interior], &locators, &mut warnings);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(placements.len(), 1);
        assert_eq!(placements[0].transform.translation, [-0.683, 2.705, -1.862]);
        assert_eq!(placements[0].variant.as_deref(), Some("s_h_std_eu"));

        let none = interior_placements(&[&interior], &locators[..1], &mut warnings);
        assert!(none.is_empty());
        assert_eq!(warnings.len(), 1, "a missing locator is reported");
    }

    #[test]
    fn composes_parent_and_child_transforms() {
        let half = std::f32::consts::FRAC_1_SQRT_2;
        // Parent at (1, 2, 3), turned 90 degrees about +y; child 1 m along
        // the parent's +x lands 1 m along world -z.
        let parent = Transform {
            translation: [1.0, 2.0, 3.0],
            rotation: [0.0, half, 0.0, half],
            scale: 1.0,
        };
        let child = Transform {
            translation: [1.0, 0.0, 0.0],
            ..Transform::IDENTITY
        };
        let world = parent.compose(&child);
        let close = |a: [f32; 3], b: [f32; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-5);
        assert!(
            close(world.translation, [1.0, 2.0, 2.0]),
            "{:?}",
            world.translation
        );
        assert_eq!(Transform::IDENTITY.compose(&child), child);
        let twice = parent.compose(&parent);
        // Two 90 degree turns are 180 degrees about +y: (0, 1, 0, 0).
        assert!(
            (twice.rotation[1].abs() - 1.0).abs() < 1e-5,
            "{:?}",
            twice.rotation
        );
    }

    #[test]
    fn rotates_by_quaternion() {
        // 90 degrees about +y maps +x to -z.
        let half = std::f32::consts::FRAC_1_SQRT_2;
        let v = rotate([0.0, half, 0.0, half], [1.0, 0.0, 0.0]);
        assert!(v[0].abs() < 1e-6 && (v[2] + 1.0).abs() < 1e-6, "{v:?}");
    }

    #[test]
    fn transforms_boxes() {
        let aabb = Aabb {
            min: [-0.25, 0.0, 0.0],
            max: [0.25, 0.5, 1.0],
        };
        let transform = Transform {
            translation: [1.0, 2.0, 3.0],
            ..Transform::IDENTITY
        };
        let world = transform_aabb(&aabb, &transform);
        assert_eq!(world.min, [0.75, 2.0, 3.0]);
        assert_eq!(world.max, [1.25, 2.5, 4.0]);
    }
}
