//! Builds `specsheet_core` catalogs from `/def/vehicle/truck/<truck>/` and the
//! preset configurations from `/def/vehicle/truck_desktop/` (findings F3,
//! F6, F15 in `docs/REVERSE_ENGINEERING.md`).

use std::collections::HashMap;

use specsheet_core::{
    slots, Accessory, AccessoryId, AccessoryKind, Configuration, Flake, Flip, Hookup, ModelRef,
    PaintJob, PaintLocks, PaintOverride, PartPaint, SlotId, TruckCatalog, TruckId, WheelPart,
};
use specsheet_formats::sii::{SiiDocument, Unit, Value};

use crate::error::{GameDataError, Result};
use crate::fs::GameFs;

const TRUCK_DEFS: &str = "/def/vehicle/truck";
const PRESET_DEFS: &str = "/def/vehicle/truck_desktop";
/// Hookup catalogue: `addon_hookup_storage.sii` plus one file per DLC,
/// each including `addon_hookups/*.sui` (finding F11).
const HOOKUP_DIR: &str = "/def/vehicle";
const HOOKUP_STORAGE: &str = "addon_hookup_storage";
/// Directory holding addon slots: `accessory/<slot>/<item>.sii`.
const ADDON_DIR: &str = "accessory";
/// Per-part paint files: `paint_job/accessory/<paint job token>.sii` (F15).
const PAINT_PARTS_DIR: &str = "paint_job/accessory";
/// Shared wheel part groups under `/def/vehicle/<group>/`, used by every
/// truck (`t_wheel` is for trailers and therefore excluded).
const WHEEL_GROUPS: &[&str] = &[
    "f_tire", "f_disc", "f_rim", "f_hub", "f_nuts", "f_cover", "f_wheel", "r_tire", "r_disc",
    "r_rim", "r_hub", "r_nuts", "r_cover", "r_wheel",
];

/// A truck model available in the installation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TruckSummary {
    pub id: TruckId,
    /// Localisation keys of brand and series, e.g. `@@brand_scania@@`.
    pub info: Vec<String>,
}

/// A definition that could not be turned into an accessory. Loading
/// continues; these are reported so broken data is visible, not fatal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadWarning {
    pub path: String,
    pub message: String,
}

/// Every truck with a `data.sii`, sorted by id.
pub fn list_trucks(fs: &GameFs) -> Result<Vec<TruckSummary>> {
    let mut trucks = Vec::new();
    for dir in fs.list_dir(TRUCK_DEFS)?.directories {
        let data_path = format!("{TRUCK_DEFS}/{dir}/data.sii");
        if !fs.exists(&data_path) {
            continue;
        }
        let doc = fs.read_sii(&data_path)?;
        let info = doc
            .units_of_class("accessory_truck_data")
            .next()
            .map(|unit| {
                unit.array_str("info")
                    .into_iter()
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        trucks.push(TruckSummary {
            id: TruckId::new(dir),
            info,
        });
    }
    trucks.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(trucks)
}

/// Loads every accessory of `truck`.
pub fn load_catalog(fs: &GameFs, truck: &TruckId) -> Result<(TruckCatalog, Vec<LoadWarning>)> {
    let root = format!("{TRUCK_DEFS}/{truck}");
    let data_path = format!("{root}/data.sii");
    let data = fs.read_sii(&data_path)?;
    let truck_unit = data
        .units_of_class("accessory_truck_data")
        .next()
        .ok_or_else(|| GameDataError::Definition {
            path: data_path.clone(),
            message: "no accessory_truck_data unit".to_owned(),
        })?;
    let info = owned(truck_unit.array_str("info"));
    let required_slots = truck_unit
        .array_str("require")
        .into_iter()
        .map(SlotId::from)
        .collect();
    let fallbacks = truck_unit
        .array_str("fallback")
        .into_iter()
        .filter_map(|entry| resolve_fallback(fs, &root, entry))
        .collect();

    let mut accessories = Vec::new();
    let mut warnings = Vec::new();
    for group in WHEEL_GROUPS {
        let dir = format!("/def/vehicle/{group}");
        let slot = SlotId::from(*group);
        let Ok(listing) = fs.list_dir(&dir) else {
            continue;
        };
        for file in listing.files.iter().filter(|f| f.ends_with(".sii")) {
            let path = format!("{dir}/{file}");
            match fs.read_sii(&path) {
                Ok(doc) => accessories.extend(
                    doc.units
                        .iter()
                        .filter_map(|unit| accessory_from_unit(unit, &slot, &path)),
                ),
                Err(error) => warnings.push(LoadWarning {
                    path,
                    message: error.to_string(),
                }),
            }
        }
    }
    // Per-part paint units by the file stem they are stored under.
    let mut paint_parts: HashMap<String, Vec<RawPart>> = HashMap::new();
    let paint_parts_dir = format!("{root}/{PAINT_PARTS_DIR}");
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let listing = fs.list_dir(&dir)?;
        stack.extend(listing.directories.iter().map(|d| format!("{dir}/{d}")));
        let Some(slot) = slot_for_dir(&root, &dir) else {
            continue;
        };
        let parts_dir = dir.eq_ignore_ascii_case(&paint_parts_dir);
        for file in listing.files.iter().filter(|f| f.ends_with(".sii")) {
            let path = format!("{dir}/{file}");
            match fs.read_sii(&path) {
                Ok(doc) if parts_dir => {
                    let stem = file.trim_end_matches(".sii").to_ascii_lowercase();
                    paint_parts
                        .entry(stem)
                        .or_default()
                        .extend(doc.units.iter().filter_map(raw_part));
                }
                // The rarer per-accessory files
                // (`accessory/<slot>/paint_job/*.sii`) hold only
                // `simple_paint_job_data` and fall out here (F15).
                Ok(doc) => accessories.extend(
                    doc.units
                        .iter()
                        .filter_map(|unit| accessory_from_unit(unit, &slot, &path)),
                ),
                Err(error) => warnings.push(LoadWarning {
                    path,
                    message: error.to_string(),
                }),
            }
        }
    }

    pair_uk_interiors(&mut accessories);
    attach_paint_parts(&mut accessories, truck, &paint_parts);
    let catalog = TruckCatalog::new(truck.clone(), info, required_slots, fallbacks, accessories);
    // Only hookups that fit one of this truck's slots are relevant.
    let slots = catalog.slots();
    let hookups: Vec<Hookup> = load_hookups(fs, &mut warnings)?
        .into_iter()
        .filter(|h| h.hosts.iter().any(|host| slots.contains(host)))
        .collect();
    Ok((catalog.with_hookups(hookups), warnings))
}

/// Gives every interior the exterior-view model of its right-hand-drive
/// twin as `exterior_uk` / `variant_uk`, and links the two as each other's
/// `drive_twin` (finding F10). The twin lives in the
/// same directory and its file name is the interior's with one extra `uk`
/// word (`uk_a.sii`, `a_uk.sii`, `standard_uk_low.sii` for `standard_low`).
/// This pairs all 219 interiors of 1.60 one-to-one.
fn pair_uk_interiors(accessories: &mut [Accessory]) {
    let key = |path: &str| -> Option<(String, Vec<String>)> {
        let (dir, file) = path.rsplit_once('/')?;
        let stem = file.strip_suffix(".sii")?;
        Some((dir.to_owned(), stem.split('_').map(str::to_owned).collect()))
    };
    // EU key (directory, words without one `uk`) -> UK twin's model and variant.
    type Key = (String, Vec<String>);
    let mut twins: HashMap<Key, (Option<String>, Option<String>, AccessoryId)> = HashMap::new();
    for accessory in accessories
        .iter()
        .filter(|a| a.kind == AccessoryKind::Interior)
    {
        let Some((dir, words)) = key(&accessory.def_path) else {
            continue;
        };
        for (index, _) in words.iter().enumerate().filter(|(_, w)| *w == "uk") {
            let mut eu_words = words.clone();
            eu_words.remove(index);
            twins.insert(
                (dir.clone(), eu_words),
                (
                    accessory.model.exterior.clone(),
                    accessory.model.variant.clone(),
                    accessory.id.clone(),
                ),
            );
        }
    }
    for accessory in accessories
        .iter_mut()
        .filter(|a| a.kind == AccessoryKind::Interior)
    {
        let Some(twin) = key(&accessory.def_path).and_then(|k| twins.get(&k)) else {
            continue;
        };
        accessory.model.exterior_uk = twin.0.clone();
        accessory.model.variant_uk = twin.1.clone();
        accessory.drive_twin = Some(twin.2.clone());
    }
    // The UK units point back at their EU twin.
    let back: HashMap<AccessoryId, AccessoryId> = accessories
        .iter()
        .filter_map(|a| Some((a.drive_twin.clone()?, a.id.clone())))
        .collect();
    for accessory in accessories.iter_mut() {
        if let Some(eu) = back.get(&accessory.id) {
            accessory.drive_twin = Some(eu.clone());
        }
    }
}

/// One `simple_paint_job_data` unit before its `acc_list[]` is resolved.
#[derive(Clone, Debug, Default, PartialEq)]
struct RawPart {
    acc_list: Vec<String>,
    mask: Option<String>,
    flake_uvscale: Option<f32>,
    flake_vratio: Option<f32>,
}

fn raw_part(unit: &Unit) -> Option<RawPart> {
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
fn attach_paint_parts(
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
fn paint_job_from_unit(unit: &Unit) -> PaintJob {
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
fn color3(value: &Value) -> Option<[f32; 3]> {
    let floats = value.as_floats()?;
    <[f32; 3]>::try_from(floats.get(..3)?).ok()
}

/// The six colours of a preset's or save's `vehicle_paint_job_accessory`
/// (F15). A key that is present but not a colour is reported.
fn paint_override(unit: &Unit, mut warn: impl FnMut(String)) -> PaintOverride {
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

/// Every hookup unit of the installation.
fn load_hookups(fs: &GameFs, warnings: &mut Vec<LoadWarning>) -> Result<Vec<Hookup>> {
    let mut hookups = Vec::new();
    for file in fs.list_dir(HOOKUP_DIR)?.files {
        if !(file.starts_with(HOOKUP_STORAGE) && file.ends_with(".sii")) {
            continue;
        }
        let path = format!("{HOOKUP_DIR}/{file}");
        match fs.read_sii(&path) {
            Ok(doc) => hookups.extend(doc.units.iter().filter_map(hookup_from_unit)),
            Err(error) => warnings.push(LoadWarning {
                path,
                message: error.to_string(),
            }),
        }
    }
    Ok(hookups)
}

/// `accessory_hookup_data`, `accessory_horn_hookup_data`,
/// `accessory_hookup_int_data`, `accessory_hookup_int_ui_data`.
fn hookup_from_unit(unit: &Unit) -> Option<Hookup> {
    if !(unit.class.starts_with("accessory_") && unit.class.contains("hookup")) {
        return None;
    }
    Some(Hookup {
        id: AccessoryId::new(unit.name.clone()),
        name: unit
            .get_str("name")
            .map(str::to_owned)
            .unwrap_or_else(|| item_label(&unit.name)),
        icon: unit.get_str("icon").map(str::to_owned),
        price: unit.get_i64("price"),
        unlock_level: unit.get_i64("unlock"),
        model: unit
            .get_str("model")
            .or_else(|| unit.get_str("exterior_model"))
            .map(str::to_owned),
        hosts: unit
            .array_str("suitable_for")
            .into_iter()
            .map(SlotId::from)
            .collect(),
        interior: unit.class.contains("_int_"),
    })
}

/// The game's preset configuration for `truck`, if one exists. Selections
/// whose definition is not in the catalog are dropped and reported.
pub fn load_preset(
    fs: &GameFs,
    catalog: &TruckCatalog,
) -> Result<Option<(Configuration, Vec<LoadWarning>)>> {
    let data_path = format!("{TRUCK_DEFS}/{}/data.sii", catalog.truck);
    for file in fs.list_dir(PRESET_DEFS)?.files {
        let path = format!("{PRESET_DEFS}/{file}");
        let doc = fs.read_sii(&path)?;
        let for_truck = vehicle_units(&doc)
            .filter_map(|u| u.get_str("data_path"))
            .any(|p| p.eq_ignore_ascii_case(&data_path));
        if for_truck {
            return Ok(Some(config_from_vehicle(&doc, catalog, &path)));
        }
    }
    Ok(None)
}

/// Reads any vehicle configuration file (`vehicle` + `vehicle_*accessory`
/// units, as in `/def/vehicle/truck_desktop`, `truck_company`, `truck_used`)
/// for the truck of `catalog`.
pub fn load_vehicle_config(
    fs: &GameFs,
    catalog: &TruckCatalog,
    path: &str,
) -> Result<(Configuration, Vec<LoadWarning>)> {
    Ok(config_from_vehicle(&fs.read_sii(path)?, catalog, path))
}

fn vehicle_units(doc: &SiiDocument) -> impl Iterator<Item = &Unit> {
    doc.units
        .iter()
        .filter(|u| u.class.starts_with("vehicle_") && u.class.ends_with("accessory"))
}

fn config_from_vehicle(
    doc: &SiiDocument,
    catalog: &TruckCatalog,
    source: &str,
) -> (Configuration, Vec<LoadWarning>) {
    let data_path = format!("{TRUCK_DEFS}/{}/data.sii", catalog.truck);
    let mut selections = Vec::new();
    let mut hookups = Vec::new();
    let mut warnings = Vec::new();
    let warn = |warnings: &mut Vec<LoadWarning>, message: String| {
        warnings.push(LoadWarning {
            path: source.to_owned(),
            message,
        })
    };
    for unit in vehicle_units(doc) {
        let Some(path) = unit.get_str("data_path") else {
            continue;
        };
        // The truck's own `data.sii` is listed too; it is not an accessory.
        if path.eq_ignore_ascii_case(&data_path) {
            continue;
        }
        let Some(accessory) = catalog.by_def_path(path) else {
            warn(
                &mut warnings,
                format!("selection {path} is not in the catalog"),
            );
            continue;
        };
        selections.push((accessory.slot.clone(), accessory.id.clone()));
        // Hookups mounted on this accessory: `slot_name[i]` is a position
        // (`slot_NN` locator of its model), `slot_hookup[i]` the hookup unit.
        let names = unit.array_str("slot_name");
        let units = unit.array_str("slot_hookup");
        if names.len() != units.len() {
            warn(
                &mut warnings,
                format!(
                    "{path}: {} slot names for {} hookups",
                    names.len(),
                    units.len()
                ),
            );
        }
        for (position, hookup) in names.into_iter().zip(units) {
            hookups.push((
                accessory.slot.clone(),
                position.to_owned(),
                AccessoryId::from(hookup),
            ));
        }
    }
    let paint = doc
        .units_of_class("vehicle_paint_job_accessory")
        .next()
        .map(|unit| paint_override(unit, |message| warn(&mut warnings, message)))
        .unwrap_or_default();
    let mut config =
        Configuration::from_selections(catalog.truck.clone(), selections).with_hookups(hookups);
    config.set_paint_override(paint);
    (config, warnings)
}

/// The slot a definition directory provides accessories for:
/// `<root>/accessory/<slot>` for addons, `<root>/<group>` otherwise.
fn slot_for_dir(root: &str, dir: &str) -> Option<SlotId> {
    let relative = dir.strip_prefix(root)?.trim_start_matches('/');
    let mut parts = relative.split('/');
    match (parts.next(), parts.next()) {
        (Some(ADDON_DIR), Some(slot)) => Some(SlotId::from(slot)),
        (Some(ADDON_DIR), None) | (Some(""), _) | (None, _) => None,
        (Some(group), _) => Some(SlotId::from(group)),
    }
}

/// `fallback[]` entries are `"<group>|<file>"`, relative to the truck
/// directory (`chassis|4x2.sii`, `accessory|f_light_chs/shape2_blk.sii`) or,
/// for shared parts such as tyres, to `/def/vehicle/<group>/`.
fn resolve_fallback(fs: &GameFs, root: &str, entry: &str) -> Option<String> {
    let (group, file) = entry.split_once('|')?;
    let local = format!("{root}/{group}/{file}");
    if fs.exists(&local) {
        return Some(local);
    }
    let shared = format!("/def/vehicle/{group}/{file}");
    fs.exists(&shared).then_some(shared)
}

fn kind_for_class(class: &str) -> Option<AccessoryKind> {
    Some(match class {
        "accessory_chassis_data" => AccessoryKind::Chassis,
        "accessory_cabin_data" => AccessoryKind::Cabin,
        "accessory_interior_data" => AccessoryKind::Interior,
        "accessory_engine_data" => AccessoryKind::Engine,
        "accessory_transmission_data" => AccessoryKind::Transmission,
        "accessory_head_lights_data" => AccessoryKind::HeadLights,
        "accessory_paint_job_data" => AccessoryKind::PaintJob,
        "accessory_wheel_data" | "accessory_rim_data" => AccessoryKind::Wheel,
        c if c.starts_with("accessory_addon") || c == "accessory_horn_addon_data" => {
            AccessoryKind::Addon
        }
        _ => return None,
    })
}

fn accessory_from_unit(unit: &Unit, slot: &SlotId, path: &str) -> Option<Accessory> {
    let kind = kind_for_class(&unit.class)?;
    let model = if kind == AccessoryKind::Interior {
        // The full cockpit (`model`) is for the interior camera; the
        // exterior view uses the light `ext_model`, which all 438 interior
        // units carry (`research/evidence/interior-survey.txt`, E1).
        ModelRef {
            exterior: unit.get_str("ext_model").map(str::to_owned),
            exterior_uk: None,
            look: unit.get_str("ext_look").map(str::to_owned),
            variant: unit.get_str("ext_variant").map(str::to_owned),
            variant_uk: None,
            cockpit: unit.get_str("model").map(str::to_owned),
            cockpit_look: unit.get_str("look").map(str::to_owned),
            cockpit_variant: unit.get_str("variant").map(str::to_owned),
            cockpit_uk: None,
        }
    } else if kind.is_base() || kind == AccessoryKind::Wheel {
        ModelRef {
            exterior: unit
                .get_str("detail_model")
                .or_else(|| unit.get_str("model"))
                .map(str::to_owned),
            exterior_uk: None,
            look: unit.get_str("look").map(str::to_owned),
            variant: unit.get_str("variant").map(str::to_owned),
            variant_uk: unit.get_str("variant_uk").map(str::to_owned),
            ..ModelRef::default()
        }
    } else {
        ModelRef {
            exterior: unit.get_str("exterior_model").map(str::to_owned),
            exterior_uk: unit.get_str("exterior_model_uk").map(str::to_owned),
            look: unit.get_str("look").map(str::to_owned),
            variant: unit.get_str("variant").map(str::to_owned),
            variant_uk: None,
            // Interior accessories: the model seen from the driver's seat.
            cockpit: unit.get_str("interior_model").map(str::to_owned),
            cockpit_look: unit.get_str("look").map(str::to_owned),
            cockpit_variant: unit.get_str("variant").map(str::to_owned),
            cockpit_uk: unit.get_str("interior_model_uk").map(str::to_owned),
        }
    };
    // Cabins list required addon slots in `require[]`.
    let requires = if kind == AccessoryKind::Cabin {
        unit.array_str("require")
            .into_iter()
            .map(SlotId::from)
            .collect()
    } else {
        Vec::new()
    };
    Some(Accessory {
        id: AccessoryId::new(unit.name.clone()),
        slot: if kind == AccessoryKind::PaintJob {
            SlotId::from(slots::PAINT_JOB)
        } else {
            slot.clone()
        },
        kind,
        def_path: path.to_owned(),
        // A few definitions have no `name`; their item id (`s770` in
        // `s770.scania.s_2016.badge`) is the most readable fallback.
        name: unit
            .get_str("name")
            .map(str::to_owned)
            .unwrap_or_else(|| item_label(&unit.name)),
        icon: unit.get_str("icon").map(str::to_owned),
        price: unit.get_i64("price"),
        unlock_level: unit.get_i64("unlock"),
        model,
        suitable_for: unit
            .array_str("suitable_for")
            .into_iter()
            .map(AccessoryId::from)
            .collect(),
        conflict_with: unit
            .array_str("conflict_with")
            .into_iter()
            .map(AccessoryId::from)
            .collect(),
        defaults: owned(unit.array_str("defaults")),
        requires,
        base_color: unit.get("base_color").and_then(color3),
        wheel: wheel_part(unit),
        left_hand_traffic: kind == AccessoryKind::Interior
            && unit.get_bool("left_hand_traffic").unwrap_or(false),
        drive_twin: None,
        paint: (kind == AccessoryKind::PaintJob).then(|| Box::new(paint_job_from_unit(unit))),
    })
}

/// Wheel role by unit class (finding F9).
fn wheel_part(unit: &Unit) -> Option<WheelPart> {
    match unit.class.as_str() {
        "accessory_wheel_data" => Some(WheelPart::Tire),
        "accessory_rim_data" => Some(WheelPart::Rim {
            tire_shift: unit.get_f32("tire_shift"),
        }),
        _ => None,
    }
}

fn item_label(unit_name: &str) -> String {
    let item = unit_name.split('.').next().unwrap_or(unit_name);
    item.replace('_', " ").to_uppercase()
}

fn owned(items: Vec<&str>) -> Vec<String> {
    items.into_iter().map(str::to_owned).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_directories_map_to_slots() {
        let root = "/def/vehicle/truck/scania.s_2016";
        assert_eq!(
            slot_for_dir(root, "/def/vehicle/truck/scania.s_2016/accessory/s_mirror"),
            Some(SlotId::from("s_mirror"))
        );
        assert_eq!(
            slot_for_dir(root, "/def/vehicle/truck/scania.s_2016/cabin"),
            Some(SlotId::from("cabin"))
        );
        assert_eq!(
            slot_for_dir(root, "/def/vehicle/truck/scania.s_2016/accessory"),
            None
        );
        assert_eq!(slot_for_dir(root, root), None);
    }

    fn interior(path: &str, model: &str, variant: &str) -> Accessory {
        let mut unit_path = path.rsplit('/').next().unwrap_or(path).to_owned();
        unit_path.truncate(unit_path.len() - 4);
        Accessory {
            id: AccessoryId::new(unit_path),
            slot: SlotId::from(slots::INTERIOR),
            kind: AccessoryKind::Interior,
            def_path: path.to_owned(),
            name: String::new(),
            icon: None,
            price: None,
            unlock_level: None,
            model: ModelRef {
                exterior: Some(model.to_owned()),
                variant: Some(variant.to_owned()),
                ..ModelRef::default()
            },
            suitable_for: Vec::new(),
            conflict_with: Vec::new(),
            defaults: Vec::new(),
            requires: Vec::new(),
            base_color: None,
            wheel: None,
            left_hand_traffic: path.contains("uk"),
            drive_twin: None,
            paint: None,
        }
    }

    #[test]
    fn interiors_get_their_uk_twin_as_right_hand_drive_model() {
        let dir = "/def/vehicle/truck/t/interior";
        let mut items = vec![
            interior(&format!("{dir}/cab_high.sii"), "/ext.pmd", "s_h_eu"),
            interior(&format!("{dir}/uk_cab_high.sii"), "/ext.pmd", "s_h_uk"),
            interior(&format!("{dir}/slp_dyn.sii"), "/slp.pmd", "default"),
            interior(&format!("{dir}/slp_dyn_uk.sii"), "/slp.pmd", "cabin_uk"),
            interior(&format!("{dir}/standard_low.sii"), "/low.pmd", "eu"),
            interior(&format!("{dir}/standard_uk_low.sii"), "/low_uk.pmd", "uk"),
            interior(&format!("{dir}/lonely.sii"), "/l.pmd", "eu"),
        ];
        pair_uk_interiors(&mut items);
        let uk = |i: usize| {
            (
                items[i].model.exterior_uk.as_deref(),
                items[i].model.variant_uk.as_deref(),
            )
        };
        assert_eq!(uk(0), (Some("/ext.pmd"), Some("s_h_uk")), "uk_ prefix");
        assert_eq!(uk(2), (Some("/slp.pmd"), Some("cabin_uk")), "_uk suffix");
        assert_eq!(uk(4), (Some("/low_uk.pmd"), Some("uk")), "uk in the middle");
        assert_eq!(items[0].drive_twin, Some(items[1].id.clone()));
        assert_eq!(items[1].drive_twin, Some(items[0].id.clone()), "both ways");
        assert_eq!(items[6].drive_twin, None);
        assert_eq!(uk(6), (None, None), "no twin");
    }

    #[test]
    fn addon_classes_are_addons() {
        for class in [
            "accessory_addon_data",
            "accessory_addon_painted_data",
            "accessory_addon_int_data",
            "accessory_horn_addon_data",
        ] {
            assert_eq!(kind_for_class(class), Some(AccessoryKind::Addon), "{class}");
        }
        assert_eq!(kind_for_class("simple_paint_job_data"), None);
        assert_eq!(kind_for_class("physics_patch_data"), None);
    }

    fn parse(src: &str) -> SiiDocument {
        specsheet_formats::sii::parse("/def/test.sii", src.as_bytes()).expect("parse")
    }

    const PAINT_JOB: &str = r#"SiiNunit
{
accessory_paint_job_data : met_a.t.paint_job
{
	name: "@@pj_met@@"
	price: 1000
	base_color: (0.3, 0.02, 0.02)
	base_color_locked: true
	paint_job_mask: "/vehicle/truck/upgrade/paintjob/t/met/pjm_at_0x0_size_2048x2048.tobj"
	mask_r_color: (1, 0, 0)
	mask_b_color: (0, 0, 1)
	mask_b_locked: true
	airbrush: true
	flipflake: true
	flake_color: (1, 1, 1)
	flake_density: 0.7
	flake_shininess: 30
	flake_clearcoat_rolloff: 4.5
	flake_uvscale: 24
	flip_color: (0.04, 0.1, 0.4)
	flip_strength: 1.7
	flip_color_locked: true
	stock: true
	color_variant[]: (1, 0, 0)
	color_variant[]: (0, 1, 0)
	color_variant[]: (0, 0, 1)
	color_variant[]: (1, 1, 1)
	color_variant[]: (0.5, 0.5, 0.5)
	suitable_for[]: "high.t.cabin"
}
}"#;

    #[test]
    fn paint_job_attributes_parse() {
        let doc = parse(PAINT_JOB);
        let accessory = accessory_from_unit(&doc.units[0], &SlotId::from("paint_job"), "/x.sii")
            .expect("paint job");
        assert_eq!(accessory.kind, AccessoryKind::PaintJob);
        assert_eq!(accessory.base_color, Some([0.3, 0.02, 0.02]));
        let paint = accessory.paint.expect("paint data");
        assert_eq!(paint.base_color, accessory.base_color);
        assert!(paint
            .mask
            .as_deref()
            .is_some_and(|m| m.ends_with("2048x2048.tobj")));
        assert_eq!(
            paint.mask_colors,
            [Some([1.0, 0.0, 0.0]), None, Some([0.0, 0.0, 1.0])]
        );
        assert!(paint.airbrush && paint.flipflake && !paint.alternate_uvset);
        assert_eq!(paint.flake.color, Some([1.0, 1.0, 1.0]));
        assert_eq!(paint.flake.density, Some(0.7));
        assert_eq!(paint.flake.shininess, Some(30.0));
        assert_eq!(paint.flake.clearcoat_rolloff, Some(4.5));
        assert_eq!(paint.flake.uvscale, Some(24.0));
        assert_eq!(paint.flake.noise, None);
        assert_eq!(paint.flip.color, Some([0.04, 0.1, 0.4]));
        assert_eq!(paint.flip.strength, Some(1.7));
        assert_eq!(
            paint.locks,
            PaintLocks {
                base: true,
                mask: [false, false, true],
                flake: false,
                flip: true,
            }
        );
        assert!(paint.stock);
        // Groups of four; the incomplete fifth colour is dropped.
        assert_eq!(paint.color_variants.len(), 1);
        assert_eq!(paint.color_variants[0][3], [1.0, 1.0, 1.0]);
        assert!(paint.parts.is_empty());
    }

    #[test]
    fn a_plain_colour_has_no_mask_or_flags() {
        let doc = parse(
            "SiiNunit\n{\naccessory_paint_job_data : c0.t.paint_job\n{\n name: \"@@c@@\"\n base_color: (1, 1, 1)\n flake_color: (1, 1)\n}\n}",
        );
        let accessory =
            accessory_from_unit(&doc.units[0], &SlotId::from("paint_job"), "/c.sii").expect("job");
        let paint = accessory.paint.expect("paint");
        assert_eq!(paint.mask, None);
        assert!(!paint.airbrush && !paint.alternate_uvset && !paint.flipflake);
        assert_eq!(
            paint.flake.color, None,
            "a two-component colour is not a colour"
        );
        assert_eq!(paint.locks, PaintLocks::default());
    }

    #[test]
    fn simple_paint_job_units_are_not_accessories() {
        let doc = parse(PARTS);
        let slot = SlotId::from("paint_job");
        assert!(doc
            .units
            .iter()
            .all(|unit| accessory_from_unit(unit, &slot, "/p.sii").is_none()));
    }

    const PARTS: &str = r#"SiiNunit
{
simple_paint_job_data : .ovr0
{
	paint_job_mask: "/vehicle/truck/upgrade/paintjob/t/met/shared_00000000.tobj"
	flake_uvscale: 8
	acc_list[]: "sideskirt.6x2_p"
	acc_list[]: "r_bumper.gone"
}
simple_paint_job_data : .ovr1
{
	flake_uvscale: 16
	flake_vratio: 0.03125
	acc_list[]: "sunshld.stock"
}
}"#;

    fn plain(id: &str, slot: &str, kind: AccessoryKind, def_path: &str) -> Accessory {
        Accessory {
            id: AccessoryId::new(id),
            slot: SlotId::from(slot),
            kind,
            def_path: def_path.to_owned(),
            ..interior("/def/x/interior/x.sii", "/x.pmd", "default")
        }
    }

    #[test]
    fn per_part_files_are_keyed_by_the_unit_token_and_resolve_acc_list() {
        // The definition file stem (`color0m`) differs from the unit's first
        // token (`col0m_a`); the per-part file carries the token.
        let doc = parse(&PAINT_JOB.replace("met_a.t.paint_job", "col0m_a.t.paint_job"));
        let job = accessory_from_unit(
            &doc.units[0],
            &SlotId::from("paint_job"),
            "/def/vehicle/truck/t/paint_job/color0m.sii",
        )
        .expect("job");
        let mut accessories = vec![
            job,
            plain(
                "6x2_p.t.sideskirt",
                "sideskirt",
                AccessoryKind::Addon,
                "/s.sii",
            ),
            plain("stock.t.sunshld", "sunshld", AccessoryKind::Addon, "/u.sii"),
        ];
        let parts_doc = parse(PARTS);
        let parts: Vec<RawPart> = parts_doc.units.iter().filter_map(raw_part).collect();
        let decoy = vec![RawPart {
            acc_list: vec!["sunshld.stock".into()],
            ..RawPart::default()
        }];
        let map = HashMap::from([("col0m_a".to_owned(), parts), ("color0m".to_owned(), decoy)]);
        attach_paint_parts(&mut accessories, &TruckId::from("t"), &map);

        let paint = accessories[0].paint.as_ref().expect("paint");
        assert_eq!(paint.parts.len(), 2, "the token file, not the stem file");
        let ovr0 = &paint.parts[0];
        assert_eq!(
            ovr0.accessories,
            vec![(
                SlotId::from("sideskirt"),
                AccessoryId::from("6x2_p.t.sideskirt")
            )]
        );
        assert_eq!(ovr0.unresolved, vec!["r_bumper.gone".to_owned()]);
        assert_eq!(ovr0.flake_uvscale, Some(8.0));
        assert!(ovr0
            .mask
            .as_deref()
            .is_some_and(|m| m.contains("shared_00000000")));
        let ovr1 = &paint.parts[1];
        assert_eq!(ovr1.mask, None);
        assert_eq!(ovr1.flake_vratio, Some(0.03125));
        assert!(ovr1.lists(&"stock.t.sunshld".into()));
        assert_eq!(
            paint
                .part_for(&"stock.t.sunshld".into())
                .map(|p| p.flake_uvscale),
            Some(Some(16.0))
        );
    }

    #[test]
    fn presets_carry_all_six_paint_colours_and_report_broken_ones() {
        let job_path = "/def/vehicle/truck/t/paint_job/color0m.sii";
        let catalog = TruckCatalog::new(
            TruckId::from("t"),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            [plain(
                "col0m_a.t.paint_job",
                "paint_job",
                AccessoryKind::PaintJob,
                job_path,
            )],
        );
        let preset = |extra: &str| {
            parse(&format!(
                "SiiNunit\n{{\nvehicle_paint_job_accessory : .paint\n{{\n \
                 mask_r_color: (1, 0, 0)\n mask_g_color: (0, 1, 0)\n mask_b_color: (0, 0, 1)\n \
                 flake_color: (1, 1, 1)\n flip_color: (0.04, 0.1, 0.4)\n base_color: (0.4, 0.5, 1)\n \
                 {extra}\n data_path: \"{job_path}\"\n}}\n}}"
            ))
        };
        let (config, warnings) = config_from_vehicle(&preset(""), &catalog, "/p.sii");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(
            config.selected(&"paint_job".into()),
            Some(&"col0m_a.t.paint_job".into()),
            "the paint job is found by its file path"
        );
        assert_eq!(
            config.paint_override(),
            &PaintOverride {
                base: Some([0.4, 0.5, 1.0]),
                mask: [
                    Some([1.0, 0.0, 0.0]),
                    Some([0.0, 1.0, 0.0]),
                    Some([0.0, 0.0, 1.0])
                ],
                flake: Some([1.0, 1.0, 1.0]),
                flip: Some([0.04, 0.1, 0.4]),
            }
        );
        let (config, warnings) =
            config_from_vehicle(&preset("flip_color: \"blue\""), &catalog, "/p.sii");
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(config.paint_override().flip, None);
    }
}
