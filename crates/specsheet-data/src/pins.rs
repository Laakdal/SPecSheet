//! Accessory pins on the tuning screen: where the game puts each dot and
//! which way it faces (finding F5, `research/evidence/pin-attributes.txt`).
//!
//! The game builds one record per pin from the truck's locators and the pin
//! table of `/def/truck_config_scene.sii` ([`PinSpec`]):
//! * a slot's base point is the first visible locator whose name starts
//!   with the slot name ([`slot_locator`]);
//! * the pin sits at base + `pin_offset` in truck space, facing `pin_dir`;
//! * `double_sided` adds a mirrored pin (offset and direction x negated);
//! * wheels get tyre, disc, hub, cover and nut pins per wheel; hub, cover
//!   and nut pins appear only while their wheel's disc pin is open.
//!
//! Every frame a pin fades with the angle between its facing and the
//! camera's view axis ([`opacity`]).

use std::collections::{BTreeMap, BTreeSet};

use specsheet_core::SlotId;
use specsheet_formats::pmg::Locator;

use crate::assembly::Drive;
use crate::scene::PinSpec;

/// Wheel pin types in the order the game builds them. Rims and complete
/// wheels (`f_wheel`, `r_wheel`) never get a pin.
const WHEEL_PIN_TYPES: [&str; 5] = ["tire", "disc", "hub", "cover", "nuts"];
/// Wheel pins whose visibility follows their wheel's disc pin.
const DISC_LINKED: [&str; 3] = ["hub", "cover", "nuts"];
/// Token names have at most 12 characters; a slot name that long must match
/// a locator name exactly.
const TOKEN_LENGTH: usize = 12;
/// The axis a pin's rotation is applied to: identity faces forward (-z).
/// Hypothesis: consistent with zero-`pin_dir` pins (beacon, lights) being
/// visible from the front.
const FORWARD: [f32; 3] = [0.0, 0.0, -1.0];

/// A pin's base point: a locator in truck space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PinBase {
    pub position: [f32; 3],
    /// Rotation quaternion in `(w, x, y, z)` order.
    pub rotation_wxyz: [f32; 4],
}

/// Where the truck's pins start from.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PinBases {
    /// The base point of every exterior accessory slot that has a locator.
    pub slots: BTreeMap<SlotId, PinBase>,
    /// Wheel positions per wheel group (`f`, `r`), axle by axle from the
    /// front, left wheel before right.
    pub wheels: Vec<(char, Vec<[f32; 3]>)>,
}

/// One dot on the truck.
#[derive(Clone, Debug, PartialEq)]
pub struct Pin {
    pub slot: SlotId,
    /// Truck space (x right, y up, -z forward).
    pub position: [f32; 3],
    /// Unit vector the pin faces, in truck space.
    pub facing: [f32; 3],
    /// The disc pin this pin belongs to (index into the same list); it is
    /// shown only while that pin is open.
    pub link: Option<usize>,
    /// Opening this pin shows the pins linked to it.
    pub expands: bool,
}

/// The locator a slot's pin starts from: the first one whose name starts
/// with the slot name (exactly equal for 12-character names). Locators are
/// searched in model order; the game's model order is a Hypothesis.
pub fn slot_locator<'a>(
    slot: &str,
    locators: impl IntoIterator<Item = &'a Locator>,
) -> Option<&'a Locator> {
    locators.into_iter().find(|l| {
        if slot.len() >= TOKEN_LENGTH {
            l.name == slot
        } else {
            l.name.starts_with(slot)
        }
    })
}

/// The pin bases of `slots` among the visible base-model `locators`, plus
/// the wheel positions from the front (`wheel_f*`) and rear (`wheel_r*`)
/// wheel locators.
pub fn bases(
    slots: &BTreeSet<SlotId>,
    locators: &[Locator],
    front: &[&Locator],
    rear: &[&Locator],
) -> PinBases {
    let base = |l: &Locator| PinBase {
        position: l.position,
        rotation_wxyz: l.rotation_wxyz,
    };
    let wheels = |locators: &[&Locator]| {
        let mut positions: Vec<[f32; 3]> = locators.iter().map(|l| l.position).collect();
        // Axle by axle from the front, left (x < 0) before right. The
        // game's own wheel order is a Hypothesis (even = left).
        positions.sort_by(|a, b| a[2].total_cmp(&b[2]).then(a[0].total_cmp(&b[0])));
        positions
    };
    PinBases {
        slots: slots
            .iter()
            .filter_map(|slot| {
                slot_locator(slot.as_str(), locators).map(|l| (slot.clone(), base(l)))
            })
            .collect(),
        wheels: [('f', wheels(front)), ('r', wheels(rear))]
            .into_iter()
            .filter(|(_, w)| !w.is_empty())
            .collect(),
    }
}

/// The truck's exterior pins, built the game's way from `table` (the scene's
/// pin table). `slots` are pinned in key order, wheels after them.
pub fn pins(table: &[PinSpec], bases: &PinBases, drive: Drive) -> Vec<Pin> {
    let entry = |group: &str| table.iter().find(|p| p.group == group);
    // Hypothesis: the UK direction is the right-hand-drive one.
    let dir = |spec: &PinSpec| match drive {
        Drive::Right => spec.dir_uk,
        Drive::Left => spec.dir,
    };
    let mut pins = Vec::new();
    for (slot, base) in &bases.slots {
        let mut push = |position, facing| {
            pins.push(Pin {
                slot: slot.clone(),
                position,
                facing,
                link: None,
                expands: false,
            })
        };
        match entry(slot.as_str()) {
            Some(spec) => {
                let dir = dir(spec);
                if spec.double_sided {
                    push(
                        add(base.position, mirror(spec.offset)),
                        facing_of(mirror(dir)),
                    );
                }
                push(add(base.position, spec.offset), facing_of(dir));
            }
            None => push(base.position, rotate(base.rotation_wxyz, FORWARD)),
        }
    }
    // The disc pin of each wheel, by (group, wheel index).
    let mut discs: BTreeMap<(char, usize), usize> = BTreeMap::new();
    for kind in WHEEL_PIN_TYPES {
        for (group, wheels) in &bases.wheels {
            let slot = SlotId::new(format!("{group}_{kind}"));
            let spec = entry(slot.as_str());
            for (index, wheel) in wheels.iter().enumerate() {
                let left = index % 2 == 0;
                let (position, facing) = match spec {
                    Some(spec) if left => (add(*wheel, spec.offset), facing_of(dir(spec))),
                    Some(spec) if spec.double_sided => (
                        add(*wheel, mirror(spec.offset)),
                        facing_of(mirror(dir(spec))),
                    ),
                    Some(_) => continue,
                    None => (*wheel, FORWARD),
                };
                if kind == "disc" {
                    discs.insert((*group, index), pins.len());
                }
                pins.push(Pin {
                    slot: slot.clone(),
                    position,
                    facing,
                    link: if DISC_LINKED.contains(&kind) {
                        discs.get(&(*group, index)).copied()
                    } else {
                        None
                    },
                    expands: kind == "disc",
                });
            }
        }
    }
    pins
}

/// How opaque a pin facing `facing` is when the camera looks along
/// `-view_axis` (`view_axis` points from the scene towards the camera):
/// fully opaque head-on, gone at about 84 degrees (Confirmed, F5).
pub fn opacity(facing: [f32; 3], view_axis: [f32; 3]) -> f32 {
    let cos = dot(normalize(facing), normalize(view_axis));
    ((cos - 0.1) / 0.9).clamp(0.0, 1.0)
}

/// The unit vector a pin with direction `dir` faces. The game turns `dir`
/// into a heading and a pitch with no roll, so the facing is `dir` itself;
/// a zero direction leaves the identity rotation, which faces forward.
fn facing_of(dir: [f32; 3]) -> [f32; 3] {
    if dot(dir, dir) < 1e-10 {
        FORWARD
    } else {
        normalize(dir)
    }
}

fn mirror([x, y, z]: [f32; 3]) -> [f32; 3] {
    [-x, y, z]
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let length = dot(v, v).sqrt();
    if length < 1e-6 {
        return [0.0; 3];
    }
    [v[0] / length, v[1] / length, v[2] / length]
}

/// Rotates `v` by the unit quaternion `(w, x, y, z)`.
fn rotate([w, x, y, z]: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let u = [x, y, z];
    let t = cross(u, v).map(|c| 2.0 * c);
    let c = cross(u, t);
    [
        v[0] + w * t[0] + c[0],
        v[1] + w * t[1] + c[1],
        v[2] + w * t[2] + c[2],
    ]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn locator(name: &str, position: [f32; 3]) -> Locator {
        Locator {
            name: name.to_owned(),
            position,
            scale: 1.0,
            rotation_wxyz: [1.0, 0.0, 0.0, 0.0],
            hookup: None,
        }
    }

    fn spec(group: &str, offset: [f32; 3], dir: [f32; 3], double_sided: bool) -> PinSpec {
        PinSpec {
            group: group.to_owned(),
            look: "custom_acc".to_owned(),
            offset,
            dir,
            dir_uk: dir,
            double_sided,
        }
    }

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5)
    }

    #[test]
    fn a_slot_starts_from_its_first_locator_by_prefix_not_the_longest_match() {
        let locators = [
            locator("f_light_top", [0.0, 3.0, 0.0]),
            locator("f_light", [0.0, 1.0, 0.0]),
            locator("mirror_s", [0.0, 2.0, -1.0]),
        ];
        let found = slot_locator("f_light", &locators).expect("f_light");
        assert_eq!(found.name, "f_light_top", "the first prefix match wins");
        assert_eq!(
            slot_locator("mirror", &locators).map(|l| l.position),
            Some([0.0, 2.0, -1.0])
        );
        // 12-character names match exactly.
        let long = [locator("sideskrt_bar1", [0.0; 3])];
        assert!(slot_locator("sideskrt_bar", &long).is_none());
    }

    #[test]
    fn a_double_sided_pin_is_mirrored_across_the_truck() {
        let mut slots = BTreeMap::new();
        slots.insert(
            SlotId::from("mirror"),
            PinBase {
                position: [0.0, 2.0, -1.0],
                rotation_wxyz: [1.0, 0.0, 0.0, 0.0],
            },
        );
        let bases = PinBases {
            slots,
            wheels: Vec::new(),
        };
        let table = [spec("mirror", [-1.35, 0.0, 0.0], [-1.0, 0.0, -1.0], true)];
        let pins = pins(&table, &bases, Drive::Left);
        assert_eq!(pins.len(), 2);
        assert!(close(pins[0].position, [1.35, 2.0, -1.0]), "mirror first");
        assert!(close(pins[1].position, [-1.35, 2.0, -1.0]));
        let s = std::f32::consts::FRAC_1_SQRT_2;
        assert!(close(pins[0].facing, [s, 0.0, -s]));
        assert!(close(pins[1].facing, [-s, 0.0, -s]));
    }

    #[test]
    fn a_slot_without_a_table_entry_sits_on_its_locator_facing_its_way() {
        let mut slots = BTreeMap::new();
        // Turned 90 degrees about y: forward (-z) becomes left (-x).
        let half = std::f32::consts::FRAC_PI_4;
        slots.insert(
            SlotId::from("toyseat"),
            PinBase {
                position: [0.5, 1.0, 0.0],
                rotation_wxyz: [half.cos(), 0.0, half.sin(), 0.0],
            },
        );
        let bases = PinBases {
            slots,
            wheels: Vec::new(),
        };
        let pins = pins(&[], &bases, Drive::Left);
        assert_eq!(pins.len(), 1);
        assert_eq!(pins[0].position, [0.5, 1.0, 0.0]);
        assert!(close(pins[0].facing, [-1.0, 0.0, 0.0]));
    }

    #[test]
    fn each_wheel_part_gets_its_own_pin_around_every_wheel() {
        let front = [
            locator("wheel_f0", [1.0, 0.5, -3.0]),
            locator("wheel_f1", [-1.0, 0.5, -3.0]),
        ];
        let rear = [
            locator("wheel_r0", [-1.0, 0.5, 1.0]),
            locator("wheel_r1", [1.0, 0.5, 1.0]),
            locator("wheel_r2", [-1.0, 0.5, 2.4]),
            locator("wheel_r3", [1.0, 0.5, 2.4]),
        ];
        let bases = bases(
            &BTreeSet::new(),
            &[],
            &front.iter().collect::<Vec<_>>(),
            &rear.iter().collect::<Vec<_>>(),
        );
        let left = [-1.0, 0.0, 0.0];
        let table = [
            spec("f_tire", [-0.1, 0.4, 0.0], left, true),
            spec("f_rim", [-0.1, 0.0, 0.0], left, true),
            spec("f_disc", [-0.1, 0.25, 0.0], left, true),
            spec("f_hub", [-0.1, 0.0, 0.0], left, true),
            spec("r_tire", [-0.3, 0.4, 0.0], left, true),
        ];
        let pins = pins(&table, &bases, Drive::Left);
        let at =
            |slot: &str| -> Vec<&Pin> { pins.iter().filter(|p| p.slot.as_str() == slot).collect() };
        // The left front tyre pin is 0.4 m above its wheel, 0.1 m out.
        let tires = at("f_tire");
        assert_eq!(tires.len(), 2, "one per front wheel");
        assert!(close(tires[0].position, [-1.1, 0.9, -3.0]));
        assert!(close(tires[0].facing, left));
        assert!(close(tires[1].position, [1.1, 0.9, -3.0]), "mirrored");
        assert_eq!(at("r_tire").len(), 4, "every rear axle has its pins");
        assert!(at("f_rim").is_empty(), "rims never get a pin");
        // Without a table entry a wheel part still gets a plain pin.
        assert_eq!(at("f_nuts").len(), 2);
        assert_eq!(at("f_nuts")[0].position, [-1.0, 0.5, -3.0]);
        // Hub pins follow the disc pin of their own wheel.
        let discs = at("f_disc");
        assert!(discs.iter().all(|d| d.expands));
        for (hub, side) in at("f_hub").iter().zip(0..) {
            let disc = &pins[hub.link.expect("linked")];
            assert_eq!(disc.slot.as_str(), "f_disc");
            assert_eq!(
                disc.position[0].signum(),
                hub.position[0].signum(),
                "{side}"
            );
        }
    }

    #[test]
    fn a_one_sided_wheel_entry_pins_only_the_left_wheels() {
        let front = [
            locator("wheel_f0", [-1.0, 0.5, -3.0]),
            locator("wheel_f1", [1.0, 0.5, -3.0]),
        ];
        let bases = bases(
            &BTreeSet::new(),
            &[],
            &front.iter().collect::<Vec<_>>(),
            &[],
        );
        let table = [spec("f_tire", [-0.1, 0.4, 0.0], [-1.0, 0.0, 0.0], false)];
        let tires: Vec<_> = pins(&table, &bases, Drive::Left)
            .into_iter()
            .filter(|p| p.slot.as_str() == "f_tire")
            .collect();
        assert_eq!(tires.len(), 1);
        assert!(tires[0].position[0] < 0.0);
    }

    #[test]
    fn right_hand_drive_uses_the_uk_direction() {
        let mut slots = BTreeMap::new();
        slots.insert(
            SlotId::from("cab_door"),
            PinBase {
                position: [0.0; 3],
                rotation_wxyz: [1.0, 0.0, 0.0, 0.0],
            },
        );
        let bases = PinBases {
            slots,
            wheels: Vec::new(),
        };
        let mut door = spec("cab_door", [0.0; 3], [1.0, 0.0, 0.0], false);
        door.dir_uk = [-1.0, 0.0, 0.0];
        assert!(close(
            pins(&[door.clone()], &bases, Drive::Left)[0].facing,
            [1.0, 0.0, 0.0]
        ));
        assert!(close(
            pins(&[door], &bases, Drive::Right)[0].facing,
            [-1.0, 0.0, 0.0]
        ));
    }

    #[test]
    fn pins_fade_with_the_angle_to_the_camera() {
        // The game's default orbit (azimuth -2.516, elevation -0.068) looks
        // at the truck's front-left quarter.
        let (az, el) = (-2.516f32, -0.068f32);
        let view_axis = [el.cos() * az.sin(), -el.sin(), el.cos() * az.cos()];
        let left = opacity([-1.0, 0.0, 0.0], view_axis);
        assert!((0.5..0.6).contains(&left), "left side half faded: {left}");
        assert_eq!(opacity([1.0, 0.0, 0.0], view_axis), 0.0, "far side");
        assert!(opacity(FORWARD, view_axis) > 0.75, "front-facing pins show");
        assert_eq!(opacity([0.0, 0.0, 1.0], view_axis), 0.0, "rear hidden");
        assert_eq!(opacity(view_axis, view_axis), 1.0);
    }
}
