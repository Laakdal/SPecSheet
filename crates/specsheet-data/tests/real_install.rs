//! Validation against a real installation. Skipped unless `ETS2_DIR` is set.

#![allow(clippy::expect_used, clippy::unwrap_used)] // test code: panics are failures

use specsheet_data::{GameFs, GameInstall};
use specsheet_formats::{pmd::Pmd, pmg::Pmg};

fn game_fs() -> Option<GameFs> {
    std::env::var_os("ETS2_DIR")?;
    let install = GameInstall::discover().expect("ETS2_DIR is an install");
    let (vfs, failures) = install.mount().expect("mount");
    assert!(failures.is_empty(), "archives failed to open: {failures:?}");
    // Regression: unknown metadata record types (e.g. 0x07 in effect.scs)
    // used to make entries unreadable.
    for archive in vfs.archives() {
        assert!(
            archive.skipped_entries().is_empty(),
            "{}: {} entries skipped, first: {:?}",
            archive.path().display(),
            archive.skipped_entries().len(),
            archive.skipped_entries().first()
        );
    }
    Some(GameFs::new(vfs))
}

/// Recursively collects files under `dir` whose name ends with one of `exts`.
fn collect(fs: &GameFs, dir: &str, exts: &[&str], out: &mut Vec<String>) {
    let listing = fs.list_dir(dir).expect("listing");
    for name in listing.files {
        if exts.iter().any(|e| name.ends_with(e)) {
            out.push(format!("{dir}/{name}"));
        }
    }
    for sub in listing.directories {
        collect(fs, &format!("{dir}/{sub}"), exts, out);
    }
}

#[test]
fn every_vehicle_definition_parses() {
    let Some(fs) = game_fs() else { return };
    let mut files = Vec::new();
    collect(&fs, "/def/vehicle", &[".sii"], &mut files);
    assert!(files.len() > 1000, "only {} files", files.len());
    let failures: Vec<String> = files
        .iter()
        .filter_map(|f| fs.read_sii(f).err().map(|e| format!("{f}: {e}")))
        .collect();
    assert!(
        failures.is_empty(),
        "{} of {} failed:\n{}",
        failures.len(),
        files.len(),
        failures.join("\n")
    );
}

#[test]
fn every_truck_model_parses() {
    let Some(fs) = game_fs() else { return };
    let mut files = Vec::new();
    collect(&fs, "/vehicle/truck", &[".pmd"], &mut files);
    assert!(files.len() > 1000, "only {} models", files.len());
    let mut failures = Vec::new();
    for pmd_path in &files {
        let pmg_path = pmd_path.replace(".pmd", ".pmg");
        let result = fs
            .read(pmd_path)
            .map_err(|e| e.to_string())
            .and_then(|b| Pmd::parse(&b).map_err(|e| e.to_string()))
            .and_then(|pmd| {
                if !fs.exists(&pmg_path) {
                    return Ok(());
                }
                let pmg = fs
                    .read(&pmg_path)
                    .map_err(|e| e.to_string())
                    .and_then(|b| Pmg::parse(&b).map_err(|e| e.to_string()))?;
                if pmd.part_attributes.len() != pmg.parts.len() {
                    return Err(format!(
                        "pmd has {} parts, pmg {}",
                        pmd.part_attributes.len(),
                        pmg.parts.len()
                    ));
                }
                let materials = pmd.looks.first().map_or(0, |l| l.materials.len());
                if let Some(piece) = pmg.pieces.iter().find(|p| p.material as usize >= materials) {
                    return Err(format!("piece material {} >= {materials}", piece.material));
                }
                Ok(())
            });
        if let Err(e) = result {
            failures.push(format!("{pmd_path}: {e}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} failed:\n{}",
        failures.len(),
        files.len(),
        failures
            .iter()
            .take(30)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn every_preset_validates_and_assembles_without_warnings() {
    use specsheet_core::SlotId;
    use specsheet_data::assembly::{assemble, Drive};
    use specsheet_data::model::ModelCache;
    use specsheet_data::{list_trucks, load_catalog, load_preset};

    let Some(fs) = game_fs() else { return };
    let cache = ModelCache::new();
    let mut failures = Vec::new();
    for truck in list_trucks(&fs).expect("trucks") {
        let (catalog, warnings) = load_catalog(&fs, &truck.id).expect("catalog");
        assert!(warnings.is_empty(), "{}: {warnings:?}", truck.id);
        let Some((preset, dropped)) = load_preset(&fs, &catalog).expect("preset") else {
            failures.push(format!("{}: no preset", truck.id));
            continue;
        };
        assert!(dropped.is_empty(), "{}: {dropped:?}", truck.id);
        for drive in [Drive::Left, Drive::Right] {
            let assembly = assemble(&fs, &cache, &catalog, &preset, drive);
            let issues = preset.validate(&catalog, &assembly.availability);
            if !issues.is_empty() || !assembly.warnings.is_empty() {
                failures.push(format!(
                    "{} {drive:?}: {issues:?} {:?}",
                    truck.id, assembly.warnings
                ));
            }
            // The cab must not look hollow: every preset's interior has an
            // exterior-view model at the cabin's `ext_interior` locator.
            let interior = SlotId::from(specsheet_core::slots::INTERIOR);
            match assembly.placements.iter().find(|p| p.slot == interior) {
                None => failures.push(format!("{} {drive:?}: no interior placement", truck.id)),
                // Right-hand drive shows the interior's UK twin (F10).
                Some(placement) if drive == Drive::Right => {
                    let model = preset
                        .selected(&interior)
                        .and_then(|id| catalog.accessory(id))
                        .map(|a| &a.model);
                    let expected = model.and_then(|m| m.exterior_uk.as_ref());
                    if expected != Some(&placement.model) {
                        failures.push(format!(
                            "{}: RHD interior {} is not the UK twin {expected:?}",
                            truck.id, placement.model
                        ));
                    }
                }
                Some(_) => {}
            }
            // Wheel anchors form one marker per axle group.
            for anchor in assembly.anchors.iter().filter(|a| {
                catalog
                    .in_slot(&a.slot)
                    .next()
                    .is_some_and(|x| x.kind == specsheet_core::AccessoryKind::Wheel)
            }) {
                if anchor.group.is_none() {
                    failures.push(format!(
                        "{}: wheel slot {} has no group",
                        truck.id, anchor.slot
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_tuning_scene_has_a_model_and_a_camera() {
    let Some(fs) = game_fs() else { return };
    let scenes = specsheet_data::scene::list_scenes(&fs).expect("scenes");
    assert!(scenes.len() >= 2, "only {} scenes", scenes.len());
    assert!(
        scenes[0].path.ends_with("/config.sii"),
        "default garage first"
    );
    for scene in &scenes {
        let model = scene.model.as_ref().expect("scene model");
        assert!(
            fs.exists(&model.path),
            "{}: missing {}",
            scene.path,
            model.path
        );
        assert!(
            scene.camera.is_some(),
            "{}: no camera frames the truck",
            scene.path
        );
    }
}

/// Finding F9: on every preset wheel, the outer lip of the rim/disc meets
/// the tyre's outboard bead, i.e. the tyre has been moved by the rim's
/// `tire_shift`.
#[test]
fn every_preset_wheel_sits_in_its_tyre() {
    use specsheet_core::WheelPart;
    use specsheet_data::assembly::{assemble, Drive, Placement};
    use specsheet_data::model::{Model, ModelCache};
    use specsheet_data::{list_trucks, load_catalog, load_preset};

    /// Lip and bead are matched within this distance along the axle (m).
    /// The residual is at most 5 mm on the stock tyres (F9).
    const TOLERANCE: f32 = 0.006;
    /// Width of the radius band taken as the bead (tyre) or lip (rim).
    const BAND: f32 = 0.02;

    /// Outboard-most world x of the visible vertices whose radius about the
    /// model's x axis lies in `radius`. `outboard` is -1 on the left.
    fn outboard_x(
        model: &Model,
        placement: &Placement,
        radius: std::ops::Range<f32>,
        outboard: f32,
    ) -> Option<f32> {
        let variant = model.variant_index(placement.variant.as_deref());
        let t = &placement.transform;
        model
            .pmg
            .parts
            .iter()
            .enumerate()
            .filter(|(i, _)| model.pmd.is_part_visible(variant, *i))
            .flat_map(|(_, part)| &model.pmg.pieces[part.pieces.clone()])
            .flat_map(|piece| &piece.positions)
            .filter(|p| radius.contains(&(p[1] * p[1] + p[2] * p[2]).sqrt()))
            .map(|p| {
                // Rotate (x, y, z, w) quaternion, then translate; x only.
                let [qx, qy, qz, qw] = t.rotation;
                let v = p.map(|c| c * t.scale);
                let tv = [
                    2.0 * (qy * v[2] - qz * v[1]),
                    2.0 * (qz * v[0] - qx * v[2]),
                    2.0 * (qx * v[1] - qy * v[0]),
                ];
                let x = v[0] + qw * tv[0] + (qy * tv[2] - qz * tv[1]);
                (x + t.translation[0]) * outboard
            })
            .reduce(f32::max)
            .map(|x| x * outboard)
    }

    fn min_radius(model: &Model) -> f32 {
        model
            .pmg
            .pieces
            .iter()
            .flat_map(|piece| &piece.positions)
            .map(|p| (p[1] * p[1] + p[2] * p[2]).sqrt())
            .fold(f32::MAX, f32::min)
    }

    let Some(fs) = game_fs() else { return };
    let cache = ModelCache::new();
    let mut failures = Vec::new();
    let mut wheels = 0;
    for truck in list_trucks(&fs).expect("trucks") {
        let (catalog, _) = load_catalog(&fs, &truck.id).expect("catalog");
        let Some((preset, _)) = load_preset(&fs, &catalog).expect("preset") else {
            continue;
        };
        let assembly = assemble(&fs, &cache, &catalog, &preset, Drive::Left);
        let role = |p: &Placement| catalog.accessory(&p.accessory).and_then(|a| a.wheel);
        for tyre in assembly
            .placements
            .iter()
            .filter(|p| role(p) == Some(WheelPart::Tire))
        {
            let [tx, ty, tz] = tyre.transform.translation;
            let outboard = if tx < 0.0 { -1.0 } else { 1.0 };
            // The largest hub-face part on the same wheel is the rim/disc.
            let Some((rim, rim_model)) = assembly
                .placements
                .iter()
                .filter(|p| matches!(role(p), Some(WheelPart::Rim { .. })))
                .filter(|p| {
                    let [x, y, z] = p.transform.translation;
                    x * tx > 0.0 && (y - ty).abs() < 1e-3 && (z - tz).abs() < 1e-3
                })
                .filter_map(|p| Some((p, cache.load(&fs, &p.model).ok()?)))
                .max_by(|a, b| a.1.pmg.aabb.max[1].total_cmp(&b.1.pmg.aabb.max[1]))
            else {
                failures.push(format!("{} {}: tyre without rim", truck.id, tyre.accessory));
                continue;
            };
            let tyre_model = cache.load(&fs, &tyre.model).expect("tyre model");
            let min_r = min_radius(&tyre_model);
            let max_r = rim_model.pmg.aabb.max[1];
            let bead = outboard_x(&tyre_model, tyre, min_r..min_r + BAND, outboard);
            let lip = outboard_x(&rim_model, rim, max_r - BAND..f32::MAX, outboard);
            wheels += 1;
            match (bead, lip) {
                (Some(bead), Some(lip)) if (bead - lip).abs() <= TOLERANCE => {}
                _ => failures.push(format!(
                    "{} {} on {} ({:?}): bead {bead:?} lip {lip:?}",
                    truck.id, tyre.accessory, rim.accessory, tyre.variant
                )),
            }
        }
    }
    assert!(wheels > 0, "no preset wheels checked");
    assert!(
        failures.is_empty(),
        "{} of {wheels} wheels misaligned:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn configured_hookups_sit_on_their_host_model() {
    use specsheet_core::{SlotId, TruckId};
    use specsheet_data::assembly::{assemble, Drive};
    use specsheet_data::model::ModelCache;
    use specsheet_data::{load_catalog, load_vehicle_config};

    let Some(fs) = game_fs() else { return };
    let cache = ModelCache::new();
    // The game's oversize Mercedes carries two beacons (`cyl01`) on its
    // light bar: `slot_name[] = slot_4, slot_0` (finding F11).
    let (catalog, _) = load_catalog(&fs, &TruckId::from("mercedes.actros")).expect("catalog");
    assert!(
        catalog.hookup(&"cyl01.addon_hookup".into()).is_some(),
        "hookup catalogue loaded"
    );
    let path = "/def/vehicle/truck_company/oversize/mercedes_6x4.sii";
    let (config, warnings) = load_vehicle_config(&fs, &catalog, path).expect("config");
    assert!(warnings.is_empty(), "{warnings:?}");
    let host = SlotId::from("r_grill");
    assert_eq!(config.hookups(&host).map(|h| h.len()), Some(2));
    assert!(config
        .validate(&catalog, &Default::default())
        .iter()
        .all(|issue| { !matches!(issue, specsheet_core::ConfigIssue::InvalidHookup { .. }) }));

    let assembly = assemble(&fs, &cache, &catalog, &config, Drive::Left);
    assert!(
        assembly.warnings.iter().all(|w| !w.contains("hookup")),
        "{:?}",
        assembly.warnings
    );
    let beacons: Vec<_> = assembly
        .placements
        .iter()
        .filter(|p| p.accessory.as_str() == "cyl01.addon_hookup")
        .collect();
    assert_eq!(beacons.len(), 2);
    // Each beacon stands on the light bar: its origin is within the host
    // model's world bounds (a wrong transform composition lands metres off).
    let positions = &assembly.hookup_positions[&host];
    let bar = assembly
        .placements
        .iter()
        .find(|p| p.slot == host && p.accessory.as_str() != "cyl01.addon_hookup")
        .expect("light bar placed");
    let model = cache.load(&fs, &bar.model).expect("bar model");
    for beacon in &beacons {
        let local = (0..3).map(|i| beacon.transform.translation[i] - bar.transform.translation[i]);
        for (i, offset) in local.enumerate() {
            assert!(
                offset >= model.pmg.aabb.min[i] - 0.05 && offset <= model.pmg.aabb.max[i] + 0.05,
                "beacon axis {i} offset {offset} outside the bar {:?}",
                model.pmg.aabb
            );
        }
        assert!(positions
            .iter()
            .any(|p| p.position == beacon.transform.translation));
    }
}

#[test]
fn every_interior_has_a_right_hand_drive_twin() {
    use specsheet_core::AccessoryKind;
    use specsheet_data::{list_trucks, load_catalog};

    let Some(fs) = game_fs() else { return };
    let (mut interiors, mut paired) = (0, 0);
    for truck in list_trucks(&fs).expect("trucks") {
        let (catalog, _) = load_catalog(&fs, &truck.id).expect("catalog");
        for accessory in catalog
            .accessories()
            .filter(|a| a.kind == AccessoryKind::Interior)
        {
            interiors += 1;
            if accessory.model.exterior_uk.is_some() {
                paired += 1;
            }
        }
    }
    // 438 interior units: 219 left-hand-drive units, each with one UK twin
    // (finding F10). The UK units themselves have no further twin.
    assert_eq!(interiors, 438);
    assert_eq!(paired, 219);
}
