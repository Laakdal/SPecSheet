//! Rule tests on a small hand-built catalog mirroring real definitions.

use crate::*;

fn accessory(id: &str, slot: &str, kind: AccessoryKind) -> Accessory {
    let model = ModelRef {
        exterior: Some(format!("/vehicle/t/{id}.pmd")),
        ..ModelRef::default()
    };
    Accessory {
        id: AccessoryId::from(id),
        slot: SlotId::from(slot),
        kind,
        def_path: format!("/def/t/{slot}/{}.sii", id.split('.').next().unwrap_or(id)),
        name: id.to_owned(),
        icon: None,
        price: None,
        unlock_level: None,
        model,
        suitable_for: Vec::new(),
        conflict_with: Vec::new(),
        defaults: Vec::new(),
        requires: Vec::new(),
        base_color: None,
        wheel: None,
    }
}

/// Two chassis, two cabins (the high one only fits 6x2), a roof grill that
/// only fits the high cabin, two sun shields where one conflicts with a
/// front mirror.
fn catalog() -> TruckCatalog {
    let c4x2 = accessory("4x2.t.chassis", "chassis", AccessoryKind::Chassis);
    let c6x2 = accessory("6x2.t.chassis", "chassis", AccessoryKind::Chassis);
    let mut low = accessory("low.t.cabin", "cabin", AccessoryKind::Cabin);
    low.suitable_for = vec!["4x2.t.chassis".into(), "6x2.t.chassis".into()];
    let mut high = accessory("high.t.cabin", "cabin", AccessoryKind::Cabin);
    high.suitable_for = vec!["6x2.t.chassis".into()];
    let mut grill = accessory("shape1.t.r_grill", "r_grill", AccessoryKind::Addon);
    grill.suitable_for = vec!["high.t.cabin".into()];
    let mut dual = accessory("dual.t.sunshld", "sunshld", AccessoryKind::Addon);
    dual.conflict_with = vec!["shape1.t.f_mirror".into()];
    let plain = accessory("plain.t.sunshld", "sunshld", AccessoryKind::Addon);
    let f_mirror = accessory("shape1.t.f_mirror", "f_mirror", AccessoryKind::Addon);
    let mut mirror = accessory("stock.t.s_mirror", "s_mirror", AccessoryKind::Addon);
    mirror.defaults = vec!["/def/t/f_mirror/shape1.sii".into()];
    TruckCatalog::new(
        TruckId::from("t"),
        Vec::new(),
        vec![SlotId::from("s_mirror")],
        vec!["/def/t/s_mirror/stock.sii".into()],
        [c4x2, c6x2, low, high, grill, dual, plain, f_mirror, mirror],
    )
}

fn all_slots(_: &Configuration) -> SlotAvailability {
    SlotAvailability {
        available: ["r_grill", "sunshld", "f_mirror", "s_mirror"]
            .into_iter()
            .map(SlotId::from)
            .collect(),
    }
}

fn base_config() -> Configuration {
    Configuration::from_selections(
        TruckId::from("t"),
        [
            ("chassis".into(), "6x2.t.chassis".into()),
            ("cabin".into(), "high.t.cabin".into()),
            ("s_mirror".into(), "stock.t.s_mirror".into()),
        ],
    )
}

#[test]
fn suitable_for_requires_one_listed_accessory() {
    let catalog = catalog();
    let mut config = base_config();
    config
        .select(&catalog, &all_slots, &"shape1.t.r_grill".into())
        .expect("fits high cabin");

    config
        .select(&catalog, &all_slots, &"low.t.cabin".into())
        .expect("base change");
    assert_eq!(
        config.selected(&"r_grill".into()),
        None,
        "grill removed with high cabin"
    );
    let grill = catalog
        .accessory(&"shape1.t.r_grill".into())
        .expect("grill");
    assert!(matches!(
        config.check(&catalog, &all_slots(&config), grill),
        Err(Incompatibility::NotSuitable { .. })
    ));
}

#[test]
fn changing_chassis_replaces_unsuitable_cabin() {
    let catalog = catalog();
    let mut config = base_config();
    let changes = config
        .select(&catalog, &all_slots, &"4x2.t.chassis".into())
        .expect("select");
    assert_eq!(
        config.selected(&"cabin".into()),
        Some(&"low.t.cabin".into())
    );
    assert!(changes.changed_slots.contains(&SlotId::from("cabin")));
    assert!(config.validate(&catalog, &all_slots(&config)).is_empty());
}

#[test]
fn conflicts_are_checked_in_both_directions() {
    let catalog = catalog();
    let mut config = base_config();
    config
        .select(&catalog, &all_slots, &"shape1.t.f_mirror".into())
        .expect("mirror");
    let err = config
        .select(&catalog, &all_slots, &"dual.t.sunshld".into())
        .expect_err("conflict");
    assert!(matches!(
        err,
        ConfigError::Incompatible {
            reason: Incompatibility::ConflictsWith { .. },
            ..
        }
    ));

    let mut config = base_config();
    config
        .select(&catalog, &all_slots, &"dual.t.sunshld".into())
        .expect("shield");
    assert!(config
        .select(&catalog, &all_slots, &"shape1.t.f_mirror".into())
        .is_err());
}

#[test]
fn defaults_are_applied_on_selection() {
    let catalog = catalog();
    let mut config = Configuration::from_selections(
        TruckId::from("t"),
        [
            ("chassis".into(), "6x2.t.chassis".into()),
            ("cabin".into(), "low.t.cabin".into()),
        ],
    );
    config
        .select(&catalog, &all_slots, &"stock.t.s_mirror".into())
        .expect("select");
    assert_eq!(
        config.selected(&"f_mirror".into()),
        Some(&"shape1.t.f_mirror".into())
    );
}

#[test]
fn required_slots_cannot_be_cleared_and_are_refilled() {
    let catalog = catalog();
    let mut config = base_config();
    assert_eq!(
        config.clear(&catalog, &all_slots, &"s_mirror".into()),
        Err(ConfigError::RequiredSlot("s_mirror".into()))
    );
    let mut empty = Configuration::from_selections(
        TruckId::from("t"),
        [
            ("chassis".into(), "6x2.t.chassis".into()),
            ("cabin".into(), "low.t.cabin".into()),
        ],
    );
    assert!(empty.validate(&catalog, &all_slots(&empty)).contains(
        &ConfigIssue::MissingRequiredSlot {
            slot: "s_mirror".into()
        }
    ));
    empty
        .select(&catalog, &all_slots, &"plain.t.sunshld".into())
        .expect("select");
    assert_eq!(
        empty.selected(&"s_mirror".into()),
        Some(&"stock.t.s_mirror".into()),
        "filled from fallback"
    );
}

#[test]
fn addons_need_an_available_slot() {
    let catalog = catalog();
    let mut config = base_config();
    let no_slots = |_: &Configuration| SlotAvailability::default();
    let err = config
        .select(&catalog, &no_slots, &"plain.t.sunshld".into())
        .expect_err("no locator");
    assert!(matches!(
        err,
        ConfigError::Incompatible {
            reason: Incompatibility::SlotUnavailable { .. },
            ..
        }
    ));
}

#[test]
fn unknown_accessories_are_rejected_and_reported() {
    let catalog = catalog();
    let mut config = base_config();
    assert_eq!(
        config.select(&catalog, &all_slots, &"nope".into()),
        Err(ConfigError::UnknownAccessory("nope".into()))
    );
    let broken = Configuration::from_selections(TruckId::from("t"), [("x".into(), "ghost".into())]);
    assert!(broken.validate(&catalog, &all_slots(&broken)).contains(
        &ConfigIssue::UnknownAccessory {
            accessory: "ghost".into()
        }
    ));
}

#[test]
fn interior_only_addons_ignore_exterior_slot_availability() {
    let mut catalog_items: Vec<Accessory> = catalog().accessories().cloned().collect();
    let mut toy = accessory("cat.t.toyseat", "toyseat", AccessoryKind::Addon);
    toy.model.exterior = None;
    catalog_items.push(toy);
    let catalog = TruckCatalog::new(
        TruckId::from("t"),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        catalog_items,
    );
    let mut config = base_config();
    let no_slots = |_: &Configuration| SlotAvailability::default();
    config
        .select(&catalog, &no_slots, &"cat.t.toyseat".into())
        .expect("interior-only addon");
}

#[test]
fn suitable_for_accepts_wildcard_patterns() {
    let mut items: Vec<Accessory> = catalog().accessories().cloned().collect();
    let mut cover = accessory("etech.fcover", "f_cover", AccessoryKind::Wheel);
    cover.suitable_for = vec!["*.t.chassis".into()];
    items.push(cover);
    let catalog = TruckCatalog::new(
        TruckId::from("t"),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        items,
    );
    let mut config = base_config();
    config
        .select(&catalog, &all_slots, &"etech.fcover".into())
        .expect("wildcard suits chassis");
}

#[test]
fn base_parts_must_satisfy_their_own_suitable_for() {
    // Regression: the 8x4-only style cabin could be selected on a 4x2 chassis
    // because base parts skipped the compatibility check.
    let catalog = catalog();
    let mut config = Configuration::from_selections(
        TruckId::from("t"),
        [
            ("chassis".into(), "4x2.t.chassis".into()),
            ("cabin".into(), "low.t.cabin".into()),
            ("s_mirror".into(), "stock.t.s_mirror".into()),
        ],
    );
    let err = config
        .select(&catalog, &all_slots, &"high.t.cabin".into())
        .expect_err("high cabin needs the 6x2 chassis");
    assert!(matches!(
        err,
        ConfigError::Incompatible {
            reason: Incompatibility::NotSuitable { .. },
            ..
        }
    ));
    assert_eq!(
        config.selected(&"cabin".into()),
        Some(&"low.t.cabin".into())
    );
}

#[test]
fn selecting_a_paint_job_clears_the_colour_override() {
    let mut items: Vec<Accessory> = catalog().accessories().cloned().collect();
    let mut red = accessory("red.t.paint_job", "paint_job", AccessoryKind::PaintJob);
    red.base_color = Some([1.0, 0.0, 0.0]);
    red.model.exterior = None;
    items.push(red);
    let catalog = TruckCatalog::new(
        TruckId::from("t"),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        items,
    );
    let mut config = base_config();
    config.set_paint_color(Some([0.0, 0.0, 1.0]));
    config
        .select(&catalog, &all_slots, &"red.t.paint_job".into())
        .expect("paint");
    assert_eq!(config.paint_color(&catalog), Some([1.0, 0.0, 0.0]));
}

fn hookup(id: &str, hosts: &[&str]) -> Hookup {
    Hookup {
        id: AccessoryId::from(id),
        name: id.to_owned(),
        icon: None,
        price: None,
        unlock_level: None,
        model: Some(format!("/vehicle/truck/upgrade/{id}.pmd")),
        hosts: hosts.iter().map(|h| SlotId::from(*h)).collect(),
        interior: false,
    }
}

fn hookup_catalog() -> TruckCatalog {
    let items: Vec<Accessory> = catalog().accessories().cloned().collect();
    TruckCatalog::new(
        TruckId::from("t"),
        Vec::new(),
        vec![SlotId::from("s_mirror")],
        Vec::new(),
        items,
    )
    .with_hookups([
        hookup("light1.addon_hookup", &["r_grill", "f_grill"]),
        hookup("horn1.addon_hookup", &["f_grill"]),
    ])
}

fn positions() -> Vec<String> {
    vec!["slot_0".to_owned(), "slot_1".to_owned()]
}

#[test]
fn hookups_mount_on_suitable_host_positions() {
    let catalog = hookup_catalog();
    let mut config = base_config();
    config
        .select(&catalog, &all_slots, &"shape1.t.r_grill".into())
        .expect("grill");
    let host = SlotId::from("r_grill");
    config
        .set_hookup(
            &catalog,
            &host,
            "slot_1",
            &positions(),
            Some(&"light1.addon_hookup".into()),
        )
        .expect("light fits r_grill");
    assert_eq!(
        config.hookups(&host).and_then(|h| h.get("slot_1")),
        Some(&"light1.addon_hookup".into())
    );
    assert!(config.validate(&catalog, &all_slots(&config)).is_empty());

    let err = config
        .set_hookup(
            &catalog,
            &host,
            "slot_0",
            &positions(),
            Some(&"horn1.addon_hookup".into()),
        )
        .expect_err("horn1 only fits f_grill");
    assert!(matches!(
        err,
        ConfigError::Hookup {
            problem: HookupProblem::NotSuitable,
            ..
        }
    ));
    let err = config
        .set_hookup(
            &catalog,
            &host,
            "slot_9",
            &positions(),
            Some(&"light1.addon_hookup".into()),
        )
        .expect_err("no such position");
    assert!(matches!(
        err,
        ConfigError::Hookup {
            problem: HookupProblem::UnknownPosition,
            ..
        }
    ));

    config
        .set_hookup(&catalog, &host, "slot_1", &positions(), None)
        .expect("remove");
    assert_eq!(config.hookups(&host), None);
}

#[test]
fn hookups_need_a_host_and_go_away_with_it() {
    let catalog = hookup_catalog();
    let mut config = base_config();
    let host = SlotId::from("r_grill");
    let err = config
        .set_hookup(
            &catalog,
            &host,
            "slot_0",
            &positions(),
            Some(&"light1.addon_hookup".into()),
        )
        .expect_err("no light bar selected");
    assert!(matches!(
        err,
        ConfigError::Hookup {
            problem: HookupProblem::NoHost,
            ..
        }
    ));

    config
        .select(&catalog, &all_slots, &"shape1.t.r_grill".into())
        .expect("grill");
    config
        .set_hookup(
            &catalog,
            &host,
            "slot_0",
            &positions(),
            Some(&"light1.addon_hookup".into()),
        )
        .expect("mount");
    // Switching to the low cabin removes the grill (it needs the high cabin),
    // and with it the hookups mounted on the grill's model.
    config
        .select(&catalog, &all_slots, &"low.t.cabin".into())
        .expect("cabin");
    assert_eq!(config.selected(&host), None);
    assert_eq!(config.hookups(&host), None);
}

#[test]
fn validation_reports_hookups_on_missing_hosts() {
    let catalog = hookup_catalog();
    let config = base_config().with_hookups([(
        SlotId::from("r_grill"),
        "slot_0".to_owned(),
        AccessoryId::from("light1.addon_hookup"),
    )]);
    assert!(config
        .validate(&catalog, &all_slots(&config))
        .iter()
        .any(|i| matches!(
            i,
            ConfigIssue::InvalidHookup {
                problem: HookupProblem::NoHost,
                ..
            }
        )));
}
