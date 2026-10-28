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
fn the_truck_configuration_screen_has_a_scene_orbit_and_pins() {
    let Some(fs) = game_fs() else { return };
    let tuning = specsheet_data::scene::tuning_scene(&fs).expect("tuning scene");
    let model = tuning.scene.model.as_ref().expect("scene model");
    assert!(fs.exists(&model.path), "missing {}", model.path);
    let orbit = tuning.orbit.as_ref().expect("orbit camera");
    assert!(orbit.fov > 0.0 && orbit.relative_bounds[0] > 0.0);
    assert!(tuning.scene.camera.is_some());
    assert!(!tuning.pins.is_empty(), "no pin data");
    assert!(tuning
        .pins
        .iter()
        .all(|p| !p.group.is_empty() && !p.look.is_empty()));
}

#[test]
fn the_truck_configuration_screen_layouts_and_templates_load() {
    use specsheet_data::ui;
    let Some(fs) = game_fs() else { return };
    let screen = ui::load_screen(&fs, "truck.config").expect("screen");
    assert!(!screen.layouts.is_empty());
    let (templates, failed) = ui::load_templates(&fs).expect("templates");
    assert!(failed.is_empty(), "templates failed to parse: {failed:?}");
    let mut failures = Vec::new();
    for layout in &screen.layouts {
        assert!(!layout.widgets.is_empty(), "{}: no widgets", layout.path);
        for widget in &layout.widgets {
            if let Some(name) = &widget.look_template {
                if templates.class(name).is_none() {
                    failures.push(format!("{}: unknown template {name}", layout.path));
                }
            }
            if !widget.id_list.is_empty() && widget.id_list.len() != widget.value_list.len() {
                failures.push(format!("{}: {} ids vs values", layout.path, widget.name));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_ui_template_parses_and_the_tuning_screen_lays_out() {
    use specsheet_data::ui::{self, flow};
    use specsheet_formats::ui_markup;
    let Some(fs) = game_fs() else { return };
    let (templates, _) = ui::load_templates(&fs).expect("templates");
    let mut failures: Vec<String> = templates
        .markups()
        .filter_map(|(name, markup)| {
            ui_markup::parse(markup)
                .err()
                .map(|e| format!("{name}: {e}"))
        })
        .collect();

    /// Real image sizes; text 8 units per character.
    struct Real<'a> {
        fs: &'a specsheet_data::GameFs,
        missing: Vec<String>,
    }
    impl flow::Measure for Real<'_> {
        fn image_size(&mut self, src: &str) -> Option<[f32; 2]> {
            let size = ui::load_ui_material(self.fs, src)
                .ok()
                .and_then(|m| m.texture)
                .and_then(|t| specsheet_data::render::load_texture(self.fs, &t).ok())
                .and_then(|t| t.mips.first().map(|m| [m.width as f32, m.height as f32]));
            if size.is_none() {
                self.missing.push(src.to_owned());
            }
            size
        }
        fn text_size(&mut self, text: &str, font: &flow::Font) -> [f32; 2] {
            [
                8.0 * text.chars().count() as f32 * font.scale[0],
                16.0 * font.scale[1],
            ]
        }
    }
    let localization =
        specsheet_data::localization::Localization::load(&fs, "en_us").expect("locale");
    let screen = ui::load_screen(&fs, "truck.config").expect("screen");
    let mut measure = Real {
        fs: &fs,
        missing: Vec::new(),
    };
    let mut laid_out = 0;
    for layout in &screen.layouts {
        for widget in &layout.widgets {
            let Some(template) = widget
                .look_template
                .as_deref()
                .and_then(|t| templates.text(t))
            else {
                continue;
            };
            let value = widget.value.as_deref().unwrap_or_default();
            let args: Vec<&str> = value.split('|').collect();
            let markup = localization.resolve(&ui::substitute(&template.markup, &args));
            let nodes = match ui_markup::parse(&markup) {
                Ok(nodes) => nodes,
                Err(e) => {
                    failures.push(format!("{}: {e}", widget.name));
                    continue;
                }
            };
            let size = [widget.rect.width(), widget.rect.height()];
            for item in flow::layout(&nodes, size, &mut measure) {
                if item.bounds().iter().any(|v| !v.is_finite()) {
                    failures.push(format!("{}: non-finite item", widget.name));
                }
            }
            laid_out += 1;
        }
    }
    assert!(laid_out > 0);
    // Images on the screen's text widgets all exist (lists and pins are
    // filled by code and are not covered here).
    failures.extend(
        measure
            .missing
            .iter()
            .map(|src| format!("missing image {src}")),
    );
    assert!(
        failures.is_empty(),
        "{}",
        failures.join(
            "
"
        )
    );
}

#[test]
fn every_accessory_icon_material_names_an_existing_texture() {
    use specsheet_data::{list_trucks, load_catalog, ui};
    let Some(fs) = game_fs() else { return };
    let mut icons = std::collections::BTreeSet::new();
    for truck in list_trucks(&fs).expect("trucks") {
        let (catalog, _) = load_catalog(&fs, &truck.id).expect("catalog");
        icons.extend(catalog.accessories().filter_map(|a| a.icon.clone()));
    }
    assert!(!icons.is_empty());
    let mut failures = Vec::new();
    for icon in &icons {
        let path = ui::accessory_icon(icon);
        // A few definitions name icons the game does not ship; those are
        // data quirks, not parse failures.
        if !fs.exists(&path) {
            continue;
        }
        match ui::load_ui_material(&fs, &path) {
            Ok(material) => match material.texture {
                Some(texture) if fs.exists(&texture) => {}
                other => failures.push(format!("{path}: texture {other:?}")),
            },
            Err(error) => failures.push(format!("{path}: {error}")),
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

/// Finding F13: every chassis has wheel locators on exactly the axles its
/// definition declares (`powered_axle[]` has one entry per axle): one left
/// and one right locator per axle, each of wheel type front (`wheel_f*`,
/// single tyres) or rear (`wheel_r*`, twin tyres). The assembly places the
/// matching wheel parts at every such locator, so this covers 6x2, 6x4, 8x4,
/// tag, pusher and lift axles.
#[test]
fn every_chassis_has_one_wheel_pair_per_declared_axle() {
    use specsheet_core::AccessoryKind;
    use specsheet_data::model::ModelCache;
    use specsheet_data::{list_trucks, load_catalog};
    use specsheet_formats::pmg::Locator;

    /// Left and right locators of one axle differ by up to 0.5 mm along z.
    const AXLE_TOLERANCE: f32 = 0.05;

    let Some(fs) = game_fs() else { return };
    let cache = ModelCache::new();
    let (mut checked, mut multi_axle) = (0, 0);
    let mut failures = Vec::new();
    for truck in list_trucks(&fs).expect("trucks") {
        let (catalog, _) = load_catalog(&fs, &truck.id).expect("catalog");
        for chassis in catalog
            .accessories()
            .filter(|a| a.kind == AccessoryKind::Chassis)
        {
            let Some(path) = chassis.model.exterior.as_deref() else {
                continue;
            };
            let document = fs.read_sii(&chassis.def_path).expect("chassis def");
            let declared = document
                .units_of_class("accessory_chassis_data")
                .next()
                .expect("chassis unit")
                .array("powered_axle")
                .len();
            let model = cache.load(&fs, path).expect("chassis model");
            let variant = model.variant_index(chassis.model.variant.as_deref());
            let mut wheels: Vec<&Locator> = model
                .visible_locators(variant)
                .filter(|l| l.name.starts_with("wheel_"))
                .collect();
            wheels.sort_by(|a, b| a.position[2].total_cmp(&b.position[2]));
            let mut axles: Vec<Vec<&Locator>> = Vec::new();
            for wheel in wheels {
                match axles.last_mut() {
                    Some(axle)
                        if wheel.position[2] - axle[axle.len() - 1].position[2]
                            <= AXLE_TOLERANCE =>
                    {
                        axle.push(wheel);
                    }
                    _ => axles.push(vec![wheel]),
                }
            }
            let pairs = axles.iter().all(|axle| {
                axle.len() == 2
                    && axle.iter().any(|l| l.position[0] < 0.0)
                    && axle.iter().any(|l| l.position[0] > 0.0)
                    && axle
                        .iter()
                        .all(|l| l.name.starts_with("wheel_f") || l.name.starts_with("wheel_r"))
            });
            if axles.len() != declared || !pairs {
                let names: Vec<Vec<&str>> = axles
                    .iter()
                    .map(|axle| axle.iter().map(|l| l.name.as_str()).collect())
                    .collect();
                failures.push(format!(
                    "{}: {declared} axles declared, wheel locators {names:?}",
                    chassis.id
                ));
            }
            checked += 1;
            multi_axle += usize::from(declared > 2);
        }
    }
    assert!(checked >= 160, "only {checked} chassis");
    assert!(multi_axle >= 100, "only {multi_axle} multi-axle chassis");
    assert!(
        failures.is_empty(),
        "{} of {checked} chassis:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn every_slot_with_a_choice_is_reachable_on_the_tuning_screen() {
    use specsheet_data::assembly::Drive;
    use specsheet_data::session::GameSession;
    use specsheet_data::view;
    if std::env::var_os("ETS2_DIR").is_none() {
        return;
    }
    let install = GameInstall::discover().expect("install");
    let game = GameSession::open(install).expect("open");
    let mut failures = Vec::new();
    for truck in view::trucks(&game) {
        let mut session = game.start_truck(&truck.id).expect("truck");
        for drive in [Drive::Left, Drive::Right] {
            session.set_drive(&game, drive);
            let overview = view::overview(&game, &session);
            // Accessory tabs reach every slot through their list; a base tab
            // opens one slot, so it may hold only one slot with a choice.
            for slot in overview.general.iter().filter(|s| s.option_count > 1) {
                let listed = view::tab_slots(&overview, slot.tab)
                    .iter()
                    .any(|t| t.slot == &slot.slot);
                let base_slot = overview
                    .general
                    .iter()
                    .find(|s| s.tab == slot.tab && s.option_count > 1)
                    .map(|s| &s.slot);
                let reachable = if slot.tab.has_pins() {
                    listed
                } else {
                    base_slot == Some(&slot.slot)
                };
                if !reachable {
                    failures.push(format!("{} {drive:?}: {}", truck.id, slot.slot));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} unreachable:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn choosing_an_interior_chooses_the_drive_side() {
    use specsheet_core::AccessoryKind;
    use specsheet_data::assembly::Drive;
    use specsheet_data::session::GameSession;
    use specsheet_data::view;
    if std::env::var_os("ETS2_DIR").is_none() {
        return;
    }
    let game = GameSession::open(GameInstall::discover().expect("install")).expect("open");
    let mut failures = Vec::new();
    let mut checked = 0;
    for truck in view::trucks(&game) {
        let mut session = game.start_truck(&truck.id).expect("truck");
        let uk = session
            .catalog()
            .accessories()
            .find(|a| a.kind == AccessoryKind::Interior && a.left_hand_traffic)
            .map(|a| a.id.clone());
        let Some(uk) = uk else { continue };
        if session.select(&game, &uk).is_err() {
            continue; // not suitable for this preset's cabin
        }
        checked += 1;
        if session.drive() != Drive::Right {
            failures.push(format!("{}: {uk} left the truck LHD", truck.id));
        }
        // Back to LHD: the interior follows to its EU twin.
        session.set_drive(&game, Drive::Left);
        let interior = session
            .config()
            .selections()
            .values()
            .filter_map(|id| session.catalog().accessory(id))
            .find(|a| a.kind == AccessoryKind::Interior);
        if interior.is_some_and(|a| a.left_hand_traffic) {
            failures.push(format!("{}: LHD kept the UK interior", truck.id));
        }
    }
    assert!(checked > 0, "no truck offers a UK interior");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_preset_has_a_cockpit_at_the_eye_point() {
    use specsheet_data::assembly::Drive;
    use specsheet_data::session::GameSession;
    use specsheet_data::view;
    if std::env::var_os("ETS2_DIR").is_none() {
        return;
    }
    let game = GameSession::open(GameInstall::discover().expect("install")).expect("open");
    let mut failures = Vec::new();
    for truck in view::trucks(&game) {
        let mut session = game.start_truck(&truck.id).expect("truck");
        for drive in [Drive::Left, Drive::Right] {
            session.set_drive(&game, drive);
            match &session.assembly().cockpit {
                None => failures.push(format!("{} {drive:?}: no cockpit", truck.id)),
                Some(cockpit) => {
                    if game.models().load(game.fs(), &cockpit.model).is_err() {
                        failures.push(format!(
                            "{} {drive:?}: {} fails to load",
                            truck.id, cockpit.model
                        ));
                    }
                    // The exterior-view interior shares the eye point.
                    let exterior = session
                        .assembly()
                        .placements
                        .iter()
                        .find(|p| p.accessory == cockpit.accessory);
                    if exterior.is_none_or(|p| p.transform != cockpit.transform) {
                        failures.push(format!("{} {drive:?}: cockpit off the eye point", truck.id));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Every truck's catalog, in truck order (paint-job tests, F15).
fn paint_catalogs(fs: &GameFs) -> Vec<specsheet_core::TruckCatalog> {
    use specsheet_data::{list_trucks, load_catalog};
    list_trucks(fs)
        .expect("trucks")
        .iter()
        .map(|truck| load_catalog(fs, &truck.id).expect("catalog").0)
        .collect()
}

#[test]
fn every_paint_job_parses() {
    use specsheet_core::AccessoryKind;
    let Some(fs) = game_fs() else { return };
    let (mut jobs, mut airbrush, mut alt_uv, mut flipflake, mut masked) = (0, 0, 0, 0, 0);
    let mut missing = Vec::new();
    for catalog in paint_catalogs(&fs) {
        for job in catalog
            .accessories()
            .filter(|a| a.kind == AccessoryKind::PaintJob)
        {
            jobs += 1;
            let Some(paint) = job.paint.as_deref() else {
                missing.push(job.id.to_string());
                continue;
            };
            assert_eq!(paint.base_color, job.base_color, "{}", job.id);
            airbrush += usize::from(paint.airbrush);
            alt_uv += usize::from(paint.alternate_uvset);
            flipflake += usize::from(paint.flipflake);
            masked += usize::from(paint.mask.is_some());
        }
    }
    println!(
        "{jobs} paint jobs: airbrush {airbrush}, alternate_uvset {alt_uv}, \
         flipflake {flipflake}, mask {masked}"
    );
    assert!(missing.is_empty(), "no paint data: {missing:?}");
    // research/evidence/paint-job-survey.txt, S2.
    assert_eq!(
        (jobs, airbrush, alt_uv, flipflake, masked),
        (19_998, 14_937, 5_487, 9_526, 18_137)
    );
}

#[test]
fn paint_job_parts_resolve_by_unit_token() {
    use specsheet_core::{AccessoryId, AccessoryKind, TruckId};
    use specsheet_data::load_catalog;
    use std::collections::BTreeSet;

    let Some(fs) = game_fs() else { return };
    let (mut resolved, mut unresolved, mut with_parts) = (0usize, Vec::new(), 0usize);
    for catalog in paint_catalogs(&fs) {
        for job in catalog.accessories().filter_map(|a| a.paint.as_deref()) {
            with_parts += usize::from(!job.parts.is_empty());
            for part in &job.parts {
                resolved += part.accessories.len();
                unresolved.extend(
                    part.unresolved
                        .iter()
                        .map(|e| format!("{}: {e}", catalog.truck)),
                );
                for (slot, id) in &part.accessories {
                    let accessory = catalog.accessory(id).expect("resolved accessory");
                    assert_eq!(&accessory.slot, slot, "{id}");
                }
            }
        }
    }
    println!(
        "{with_parts} paint jobs with parts; acc_list {resolved} resolved, {} unresolved: {:?}",
        unresolved.len(),
        unresolved.iter().take(20).collect::<Vec<_>>()
    );
    // S3, S4: 17,410 paint jobs have a per-part file; 560,896 of 561,084
    // entries resolve.
    assert!(with_parts >= 17_000, "{with_parts}");
    assert!(resolved >= 560_000, "{resolved}");
    assert!(unresolved.len() * 1000 <= resolved, "{}", unresolved.len());

    // Distinct accessories listed by the parts of one paint job (S4).
    let listed = |truck: &str, job: &str| -> (usize, usize) {
        let (catalog, _) = load_catalog(&fs, &TruckId::from(truck)).expect("catalog");
        let id = AccessoryId::new(format!("{job}.{truck}.paint_job"));
        let paint = catalog
            .accessory(&id)
            .and_then(|a| a.paint.as_deref())
            .unwrap_or_else(|| panic!("{id} has paint data"));
        let accessories: BTreeSet<&AccessoryId> = paint
            .parts
            .iter()
            .flat_map(|p| p.accessories.iter().map(|(_, a)| a))
            .collect();
        let missing: usize = paint.parts.iter().map(|p| p.unresolved.len()).sum();
        assert!(
            accessories.iter().all(|a| catalog
                .accessory(a)
                .is_some_and(|a| a.kind != AccessoryKind::PaintJob)),
            "{id}"
        );
        (accessories.len(), missing)
    };
    assert_eq!(listed("scania.s_2016", "770s_met_a"), (64, 0));
    assert_eq!(listed("volvo.fh_2024", "academy_a"), (51, 0));
    assert_eq!(listed("volvo.fh_2024", "v_m_inb_a"), (51, 0));
    // `color0m.sii` holds `col0m_s_s_a`: the part file is found by the token.
    let (catalog, _) = load_catalog(&fs, &TruckId::from("scania.s_2016")).expect("catalog");
    let by_file = catalog
        .by_def_path("/def/vehicle/truck/scania.s_2016/paint_job/color0m.sii")
        .expect("paint job by file path");
    assert_eq!(by_file.id.as_str(), "col0m_s_s_a.scania.s_2016.paint_job");
}

#[test]
fn every_paint_mask_exists_or_is_a_constant() {
    use specsheet_core::shared_mask_color;
    use std::collections::BTreeSet;

    let Some(fs) = game_fs() else { return };
    let mut masks = BTreeSet::new();
    for catalog in paint_catalogs(&fs) {
        for job in catalog.accessories().filter_map(|a| a.paint.as_deref()) {
            masks.extend(job.mask.clone());
            masks.extend(job.parts.iter().filter_map(|p| p.mask.clone()));
        }
    }
    let constants = masks
        .iter()
        .filter(|m| shared_mask_color(m).is_some())
        .count();
    let missing: Vec<&String> = masks
        .iter()
        .filter(|m| shared_mask_color(m).is_none() && !fs.exists(m))
        .collect();
    println!(
        "{} distinct masks, {constants} shared_ constants, {} missing: {missing:#?}",
        masks.len(),
        missing.len()
    );
    // S5: 49,457 distinct masks, 27 referenced masks are missing.
    assert!(masks.len() >= 49_000, "{}", masks.len());
    assert!(constants >= 4_800, "{constants}");
    assert!(missing.len() <= 40, "{} masks missing", missing.len());
}

#[test]
fn every_desktop_preset_loads_its_paint_colours() {
    use specsheet_core::{AccessoryKind, TruckCatalog, TruckId};
    use specsheet_data::{load_catalog, load_vehicle_config};
    use std::collections::HashMap;

    const KEYS: [&str; 6] = [
        "base_color",
        "mask_r_color",
        "mask_g_color",
        "mask_b_color",
        "flake_color",
        "flip_color",
    ];
    let Some(fs) = game_fs() else { return };
    let dir = "/def/vehicle/truck_desktop";
    let mut catalogs: HashMap<String, TruckCatalog> = HashMap::new();
    let (mut presets, mut written, mut failures) = (0, 0, Vec::new());
    for file in fs.list_dir(dir).expect("presets").files {
        let path = format!("{dir}/{file}");
        let doc = fs.read_sii(&path).expect("preset");
        let truck = doc
            .units
            .iter()
            .filter_map(|u| u.get_str("data_path"))
            .find_map(|p| {
                p.strip_prefix("/def/vehicle/truck/")?
                    .strip_suffix("/data.sii")
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| panic!("{path}: no truck"));
        let catalog = catalogs
            .entry(truck.clone())
            .or_insert_with(|| load_catalog(&fs, &TruckId::new(truck)).expect("catalog").0);
        let (config, warnings) = load_vehicle_config(&fs, catalog, &path).expect("config");
        presets += 1;
        if !warnings.is_empty() {
            failures.push(format!("{path}: {warnings:?}"));
        }
        if config
            .paint_job(catalog)
            .is_none_or(|job| job.kind != AccessoryKind::PaintJob || job.paint.is_none())
        {
            failures.push(format!("{path}: no paint job with paint data"));
        }
        let unit = doc
            .units_of_class("vehicle_paint_job_accessory")
            .next()
            .unwrap_or_else(|| panic!("{path}: no paint unit"));
        let paint = config.paint_override();
        let loaded = [
            paint.base,
            paint.mask[0],
            paint.mask[1],
            paint.mask[2],
            paint.flake,
            paint.flip,
        ];
        for (key, value) in KEYS.iter().zip(loaded) {
            let raw = unit
                .get(key)
                .and_then(|v| v.as_floats())
                .and_then(|v| <[f32; 3]>::try_from(v.get(..3)?).ok());
            written += usize::from(raw.is_some());
            if raw != value {
                failures.push(format!("{path}: {key} {raw:?} loaded as {value:?}"));
            }
        }
    }
    println!(
        "{presets} presets write {written} of {} paint colours",
        presets * 6
    );
    assert_eq!(presets, 26, "S8: 26 truck_desktop presets");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_cabin_change_keeps_the_paint_job_design() {
    use specsheet_core::{AccessoryId, Configuration, TruckId};
    use specsheet_data::assembly::{slot_availability, Drive};
    use specsheet_data::model::ModelCache;
    use specsheet_data::{load_catalog, load_preset};

    let Some(fs) = game_fs() else { return };
    let cache = ModelCache::new();
    let (catalog, _) = load_catalog(&fs, &TruckId::from("scania.s_2016")).expect("catalog");
    let (mut config, _) = load_preset(&fs, &catalog)
        .expect("preset")
        .expect("a preset");
    let availability = |c: &Configuration| slot_availability(&fs, &cache, &catalog, c, Drive::Left);
    let job = |name: &str| AccessoryId::new(format!("{name}.scania.s_2016.paint_job"));
    // 770s_met_a/_b/_c: one design for three cabins (S1).
    let first = catalog.accessory(&job("770s_met_a")).expect("770s_met_a");
    config
        .select(&catalog, &availability, &first.suitable_for[0].clone())
        .expect("its cabin");
    config
        .select(&catalog, &availability, &first.id)
        .expect("paint job");
    config.set_paint_color(Some([0.1, 0.2, 0.3]));
    let family: Vec<_> = catalog
        .in_slot(&first.slot)
        .filter(|a| a.name == first.name && a.id != first.id)
        .collect();
    let mut moved = 0;
    for member in family {
        let Some(cabin) = member.suitable_for.first() else {
            continue;
        };
        let Some(cabin) = catalog.accessory(cabin) else {
            continue;
        };
        if config
            .check(&catalog, &availability(&config), cabin)
            .is_err()
            || config.select(&catalog, &availability, &cabin.id).is_err()
        {
            continue;
        }
        let now = config.paint_job(&catalog).expect("a paint job");
        assert_eq!(now.name, first.name, "{} on {}", now.id, cabin.id);
        assert_eq!(config.paint_color(&catalog), Some([0.1, 0.2, 0.3]));
        moved += 1;
    }
    assert!(moved > 0, "no other cabin of the family could be selected");
}
