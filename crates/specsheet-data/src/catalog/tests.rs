use std::collections::HashMap;

use specsheet_core::{AccessoryId, SlotId, TruckId};
use specsheet_formats::sii::SiiDocument;
use specsheet_truck::{
    slots, Accessory, AccessoryKind, ModelRef, PaintLocks, PaintOverride, TruckCatalog,
};

use super::accessory::{accessory_from_unit, kind_for_class, pair_uk_interiors};
use super::paint::{attach_paint_parts, raw_part, RawPart};
use super::preset::config_from_vehicle;
use super::slot_for_dir;

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
