//! Presentation-neutral views of a [`TruckSession`]: display names, which
//! slots get a marker, and plain-language verdicts. Front ends render these
//! as they are and hold no tuning rules (see `docs/ARCHITECTURE.md`).

use std::collections::{HashMap, HashSet};

use specsheet_core::colour::linear_to_srgb;
use specsheet_core::{AccessoryId, SlotId, TruckId};
use specsheet_truck::{
    Accessory, AccessoryKind, Incompatibility, DEFAULT_BASE_COLOR, DEFAULT_FLAKE_COLOR,
    DEFAULT_FLIP_COLOR,
};

use crate::{GameSession, SlotOption, TruckSession};
use specsheet_assembly::pins;
use specsheet_assembly::Drive;

/// A tab of the game's truck configuration screen (finding F5). The tab
/// row's `value_list` names them; [`Tab::from_token`] reads those names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tab {
    Cabin,
    Chassis,
    Engine,
    Transmission,
    /// Paint job (`color`).
    Color,
    /// Exterior accessories: one pin per slot on the truck.
    ExteriorAccessories,
    Interior,
    /// Interior accessories: pins inside the cab.
    InteriorAccessories,
}

impl Tab {
    /// The tab named by a `value_list` token of the tab row.
    pub fn from_token(token: &str) -> Option<Self> {
        Some(match token {
            "cabin" => Self::Cabin,
            "chassis" => Self::Chassis,
            "engine" => Self::Engine,
            "transmission" => Self::Transmission,
            "color" => Self::Color,
            "e_accessories" => Self::ExteriorAccessories,
            "interior" => Self::Interior,
            "i_accessories" => Self::InteriorAccessories,
            _ => return None,
        })
    }

    /// Accessory tabs pick a slot through pins; the others show one slot's
    /// options directly and have no pins (F5).
    pub fn has_pins(self) -> bool {
        matches!(self, Self::ExteriorAccessories | Self::InteriorAccessories)
    }

    /// Localisation key of the list caption (Hypothesis:
    /// `docs/formats/ui-truck-config.md`, "Captions").
    pub fn caption_key(self) -> &'static str {
        match self {
            Self::Cabin => "cabin_t",
            Self::Chassis => "chassis_t",
            Self::Engine => "engine_t",
            Self::Transmission => "transmission_t",
            Self::Color => "paint_job_t",
            Self::ExteriorAccessories => "accessory_t",
            Self::Interior => "interior_t",
            Self::InteriorAccessories => "in_accessory",
        }
    }
}

/// The tab a slot belongs to: base parts by kind, every other accessory
/// under the exterior or interior accessories (F5).
pub fn tab_of(kind: AccessoryKind, interior: bool) -> Tab {
    match kind {
        AccessoryKind::Cabin => Tab::Cabin,
        AccessoryKind::Chassis => Tab::Chassis,
        AccessoryKind::Engine => Tab::Engine,
        AccessoryKind::Transmission => Tab::Transmission,
        AccessoryKind::PaintJob => Tab::Color,
        AccessoryKind::Interior => Tab::Interior,
        _ if interior => Tab::InteriorAccessories,
        _ => Tab::ExteriorAccessories,
    }
}

/// Sidebar group of a hotspot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HotspotCategory {
    /// Chassis and cabin.
    Base,
    Addon,
    Wheel,
    /// Slots whose parts sit inside the cabin.
    Interior,
}

impl HotspotCategory {
    pub const ALL: [Self; 4] = [Self::Base, Self::Addon, Self::Wheel, Self::Interior];

    pub fn title(self) -> &'static str {
        match self {
            Self::Base => "Chassis & cabin",
            Self::Addon => "Exterior",
            Self::Wheel => "Wheels",
            Self::Interior => "Interior",
        }
    }
}

/// A slot with a clickable marker on the truck.
#[derive(Clone, Debug, PartialEq)]
pub struct Hotspot {
    pub slot: SlotId,
    pub label: String,
    /// Truck-space position (x right, y up, -z forward).
    pub position: [f32; 3],
    /// The unit vector the pin faces; it fades as it turns from the camera
    /// ([`pins::opacity`]). `None` for interior pins, which always show.
    pub facing: Option<[f32; 3]>,
    /// The pin (index into [`TruckOverview::hotspots`]) this one belongs to:
    /// it shows only while that pin is open (a wheel's hub, cover and nuts
    /// follow its disc, F5).
    pub link: Option<usize>,
    /// Opening this pin shows the pins linked to it.
    pub expands: bool,
    pub category: HotspotCategory,
    /// Display name of the selected accessory.
    pub selected: Option<String>,
    /// Icon of the selected accessory.
    pub selected_icon: Option<String>,
    pub option_count: usize,
    /// Nothing is placed in the slot; the anchor sits on bare locators.
    pub empty: bool,
    /// Always an accessory tab: base parts have no pins.
    pub tab: Tab,
}

/// A slot without a marker (paint job, engine, interior-only parts...).
#[derive(Clone, Debug, PartialEq)]
pub struct SlotEntry {
    pub slot: SlotId,
    pub label: String,
    pub selected: Option<String>,
    pub selected_icon: Option<String>,
    pub option_count: usize,
    pub tab: Tab,
}

/// A slot listed under a tab.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TabSlot<'a> {
    pub slot: &'a SlotId,
    pub label: &'a str,
    pub selected: Option<&'a str>,
    pub selected_icon: Option<&'a str>,
    pub option_count: usize,
    /// The slot has a pin on the truck.
    pub pinned: bool,
}

/// Every slot of `tab`, pinned or not, by label. Accessory tabs reach
/// slots without a pin (no anchor yet) through this list.
pub fn tab_slots(overview: &TruckOverview, tab: Tab) -> Vec<TabSlot<'_>> {
    let pinned = overview
        .hotspots
        .iter()
        .filter(|h| h.tab == tab)
        .map(|h| TabSlot {
            slot: &h.slot,
            label: &h.label,
            selected: h.selected.as_deref(),
            selected_icon: h.selected_icon.as_deref(),
            option_count: h.option_count,
            pinned: true,
        });
    let others = overview
        .general
        .iter()
        .filter(|s| s.tab == tab)
        .map(|s| TabSlot {
            slot: &s.slot,
            label: &s.label,
            selected: s.selected.as_deref(),
            selected_icon: s.selected_icon.as_deref(),
            option_count: s.option_count,
            pinned: false,
        });
    let mut slots: Vec<TabSlot<'_>> = pinned.chain(others).collect();
    slots.sort_by(|a, b| {
        a.label
            .cmp(b.label)
            .then(a.slot.as_str().cmp(b.slot.as_str()))
    });
    slots.dedup_by(|a, b| a.slot == b.slot);
    slots
}

/// Everything a front end shows about the truck being tuned.
#[derive(Clone, Debug, PartialEq)]
pub struct TruckOverview {
    pub truck: TruckId,
    pub name: String,
    pub drive: Drive,
    pub hotspots: Vec<Hotspot>,
    /// Slots without a marker, listed in a "General" group.
    pub general: Vec<SlotEntry>,
    /// Linear RGB paint colour of the selected paint job.
    pub paint_color: Option<[f32; 3]>,
    /// The configuration's price (`Configuration::price`).
    pub price: i64,
    pub warnings: Vec<String>,
}

/// One option of a slot with the rules' verdict.
#[derive(Clone, Debug, PartialEq)]
pub struct OptionView {
    pub id: AccessoryId,
    pub name: String,
    /// The tile template's icon argument `%1`, spliced into
    /// `<img src=/material/ui/accessory/%1>`: an icon name, or for a paint
    /// job the game's thumbnail markup (`icon_argument`).
    pub icon: Option<String>,
    pub price: Option<i64>,
    pub unlock_level: Option<i64>,
    pub selected: bool,
    pub compatible: bool,
    /// Why the option is not compatible, in plain words.
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SlotOptions {
    pub slot: SlotId,
    pub label: String,
    pub options: Vec<OptionView>,
}

/// A truck of the installation with its display name.
#[derive(Clone, Debug, PartialEq)]
pub struct TruckEntry {
    pub id: TruckId,
    pub name: String,
}

/// Every truck of the installation with its display name.
pub fn trucks(game: &GameSession) -> Vec<TruckEntry> {
    game.trucks()
        .iter()
        .map(|t| TruckEntry {
            id: t.id.clone(),
            name: truck_name(game, &t.info, t.id.as_str()),
        })
        .collect()
}

/// Truck display name from its `info[]` localisation keys.
pub fn truck_name(game: &GameSession, info: &[String], fallback: &str) -> String {
    let parts: Vec<String> = info
        .iter()
        .map(|key| game.localization().resolve(key))
        .collect();
    if parts.is_empty() {
        fallback.to_owned()
    } else {
        parts.join(" ")
    }
}

/// Slot display name: game strings use the slot name as key (e.g.
/// `r_grill` → "Light Bar"); otherwise the slot name is humanised.
pub fn slot_label(game: &GameSession, slot: &SlotId) -> String {
    game.localization()
        .get(slot.as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| humanize(slot.as_str()))
}

fn humanize(slot: &str) -> String {
    let words: Vec<String> = slot
        .split('_')
        .filter(|w| !w.is_empty())
        .map(|w| match w {
            "f" => "Front".to_owned(),
            "r" => "Rear".to_owned(),
            other => {
                let mut chars = other.chars();
                chars
                    .next()
                    .map(|c| c.to_uppercase().chain(chars).collect())
                    .unwrap_or_default()
            }
        })
        .collect();
    words.join(" ")
}

fn selected_icon(session: &TruckSession, slot: &SlotId) -> Option<String> {
    session
        .config()
        .selected(slot)
        .and_then(|id| session.catalog().accessory(id))
        .and_then(icon_argument)
}

/// What the tile template's `<img src=/material/ui/accessory/%1>` takes as
/// `%1` for `accessory`: its icon, or for a paint job without one the markup
/// the game builds from the job's own colours (F17): one tinted
/// `color_icon.mat`, or for `flipflake` jobs base, flip and flake layers
/// under the `clr_3.mat` overlay.
fn icon_argument(accessory: &Accessory) -> Option<String> {
    if let Some(icon) = accessory.icon.as_deref().filter(|i| !i.is_empty()) {
        return Some(if accessory.paint.is_some() {
            format!("{icon}.mat")
        } else {
            icon.to_owned()
        });
    }
    let paint = accessory.paint.as_deref()?;
    let base = ui_color(
        paint
            .base_color
            .or(accessory.base_color)
            .unwrap_or(DEFAULT_BASE_COLOR),
    );
    if !paint.flipflake {
        return Some(format!("color_icon.mat color={base}"));
    }
    let flip = ui_color(paint.flip.color.unwrap_or(DEFAULT_FLIP_COLOR));
    let flake = ui_color(paint.flake.color.unwrap_or(DEFAULT_FLAKE_COLOR));
    Some(format!(
        "clr_0.mat color={base}><offset hshift=-256>\
         <img src=/material/ui/clr_1.mat color={flip}><offset hshift=-256>\
         <img src=/material/ui/clr_2.mat color={flake}><offset hshift=-256>\
         <img src=/material/ui/clr_3.mat"
    ))
}

/// A linear colour as UI markup text, as the game writes it (F17): each
/// channel clamped to 0–1, sRGB-encoded, ×255 truncated, printed
/// `FF` + blue + green + red.
fn ui_color(linear: [f32; 3]) -> String {
    let [r, g, b] = linear.map(|c| (linear_to_srgb(c) * 255.0) as u8);
    format!("FF{b:02X}{g:02X}{r:02X}")
}

fn selected_name(game: &GameSession, session: &TruckSession, slot: &SlotId) -> Option<String> {
    session
        .config()
        .selected(slot)
        .and_then(|id| session.catalog().accessory(id))
        .map(|a| game.localization().resolve(&a.name))
}

fn category(kind: AccessoryKind) -> HotspotCategory {
    match kind {
        AccessoryKind::Chassis | AccessoryKind::Cabin => HotspotCategory::Base,
        AccessoryKind::Wheel => HotspotCategory::Wheel,
        _ => HotspotCategory::Addon,
    }
}

/// The current truck: markers for slots with a visible choice, the other
/// slots as a list, paint colour and warnings.
pub fn overview(game: &GameSession, session: &TruckSession) -> TruckOverview {
    let catalog = session.catalog();
    let assembly = session.assembly();
    // Which slots get a pin: accessory tabs only (base parts are picked
    // through their own tab, F5), and among them every addon and wheel slot
    // with an option, plus any other slot with a choice.
    let pinned = |slot: &SlotId, interior: bool| {
        let mut options = catalog.in_slot(slot);
        let kind = options.next()?.kind;
        let option_count = 1 + options.count();
        let tab = tab_of(kind, interior);
        let chosen =
            matches!(kind, AccessoryKind::Addon | AccessoryKind::Wheel) || option_count > 1;
        (tab.has_pins() && chosen).then_some((kind, option_count, tab))
    };
    let anchors: HashMap<&SlotId, _> = assembly.anchors.iter().map(|a| (&a.slot, a)).collect();
    let placed: HashSet<&SlotId> = assembly.placements.iter().map(|p| &p.slot).collect();
    let hotspot =
        |slot: &SlotId, position, facing, kind, option_count, tab, interior: bool| Hotspot {
            slot: slot.clone(),
            label: slot_label(game, slot),
            position,
            facing,
            link: None,
            expands: false,
            category: if interior {
                HotspotCategory::Interior
            } else {
                category(kind)
            },
            selected: selected_name(game, session, slot),
            selected_icon: selected_icon(session, slot),
            option_count,
            empty: !placed.contains(slot),
            tab,
        };
    // Exterior pins are the game's (F5); a slot whose parts sit inside the
    // cab keeps its anchor as an interior pin instead.
    let exterior = pins::pins(game.pin_table(), &assembly.pin_bases, session.drive());
    let mut kept: Vec<Option<usize>> = Vec::with_capacity(exterior.len());
    let mut hotspots: Vec<Hotspot> = Vec::new();
    for pin in &exterior {
        let inside = anchors.get(&pin.slot).is_some_and(|a| a.interior);
        let shown = pinned(&pin.slot, false).filter(|_| !inside);
        // A linked pin follows its target out when the target is dropped.
        let link = pin.link.map(|i| kept.get(i).copied().flatten());
        let index = match (shown, link) {
            (Some(_), Some(None)) | (None, _) => None,
            (Some((kind, option_count, tab)), link) => {
                let mut h = hotspot(
                    &pin.slot,
                    pin.position,
                    Some(pin.facing),
                    kind,
                    option_count,
                    tab,
                    false,
                );
                h.link = link.flatten();
                h.expands = pin.expands;
                hotspots.push(h);
                Some(hotspots.len() - 1)
            }
        };
        kept.push(index);
    }
    for anchor in assembly.anchors.iter().filter(|a| a.interior) {
        if let Some((kind, option_count, tab)) = pinned(&anchor.slot, true) {
            hotspots.push(hotspot(
                &anchor.slot,
                anchor.position,
                None,
                kind,
                option_count,
                tab,
                true,
            ));
        }
    }
    let marked: HashSet<&SlotId> = hotspots.iter().map(|h| &h.slot).collect();
    let general = catalog
        .slots()
        .into_iter()
        .filter(|slot| !marked.contains(slot))
        .map(|slot| SlotEntry {
            label: slot_label(game, &slot),
            selected: selected_name(game, session, &slot),
            selected_icon: selected_icon(session, &slot),
            option_count: catalog.in_slot(&slot).count(),
            tab: catalog
                .in_slot(&slot)
                .next()
                .map_or(Tab::ExteriorAccessories, |a| tab_of(a.kind, false)),
            slot,
        })
        .collect();
    let warnings = assembly
        .warnings
        .iter()
        .cloned()
        .chain(
            session
                .load_warnings()
                .iter()
                .map(|w| format!("{}: {}", w.path, w.message)),
        )
        .collect();
    TruckOverview {
        truck: catalog.truck.clone(),
        name: truck_name(game, &catalog.info, catalog.truck.as_str()),
        drive: session.drive(),
        hotspots,
        general,
        paint_color: session.config().paint_color(catalog),
        price: session.config().price(catalog),
        warnings,
    }
}

/// The options of `slot` in display order, with verdicts.
pub fn slot_options(game: &GameSession, session: &TruckSession, slot: &SlotId) -> SlotOptions {
    SlotOptions {
        slot: slot.clone(),
        label: slot_label(game, slot),
        options: session
            .options(slot)
            .iter()
            .map(|option| option_view(game, option))
            .collect(),
    }
}

fn option_view(game: &GameSession, option: &SlotOption<'_>) -> OptionView {
    OptionView {
        id: option.accessory.id.clone(),
        name: game.localization().resolve(&option.accessory.name),
        icon: icon_argument(option.accessory),
        price: option.accessory.price,
        unlock_level: option.accessory.unlock_level,
        selected: option.selected,
        compatible: option.verdict.is_ok(),
        reason: option.verdict.as_ref().err().map(|r| describe(game, r)),
    }
}

fn describe(game: &GameSession, reason: &Incompatibility) -> String {
    match reason {
        Incompatibility::NotSuitable { needs_one_of } => {
            let names: Vec<&str> = needs_one_of.iter().take(3).map(|id| id.as_str()).collect();
            format!("Requires one of: {}", names.join(", "))
        }
        Incompatibility::ConflictsWith { accessory } => {
            format!("Conflicts with {accessory}")
        }
        Incompatibility::SlotUnavailable { slot } => {
            format!(
                "{} is not available on this cabin/chassis",
                slot_label(game, slot)
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specsheet_truck::{Flake, Flip, ModelRef, PaintJob};

    fn paint_job(icon: Option<&str>, paint: PaintJob) -> Accessory {
        Accessory {
            id: AccessoryId::from("x.t.paint_job"),
            slot: SlotId::from("paint_job"),
            kind: AccessoryKind::PaintJob,
            def_path: "/def/t/paint_job/x.sii".to_owned(),
            name: "x".to_owned(),
            icon: icon.map(str::to_owned),
            price: None,
            unlock_level: None,
            model: ModelRef::default(),
            suitable_for: Vec::new(),
            conflict_with: Vec::new(),
            defaults: Vec::new(),
            requires: Vec::new(),
            base_color: paint.base_color,
            wheel: None,
            left_hand_traffic: false,
            drive_twin: None,
            paint: Some(Box::new(paint)),
        }
    }

    #[test]
    fn paint_jobs_without_an_icon_show_their_own_colours() {
        let plain = PaintJob {
            base_color: Some([0.5, 0.25, 1.0]),
            ..PaintJob::default()
        };
        // sRGB, ×255 truncated (0.5 → 187.5 → BB; 1.0 → 254.99998 in f32
        // → FE), written FF BB GG RR.
        assert_eq!(
            icon_argument(&paint_job(None, plain.clone())).as_deref(),
            Some("color_icon.mat color=FFFE88BB")
        );
        assert_eq!(
            icon_argument(&paint_job(Some("academy"), plain)).as_deref(),
            Some("academy.mat"),
            "an icon wins"
        );
        let metallic = PaintJob {
            flipflake: true,
            flip: Flip {
                color: Some([0.0, 0.0, 1.0]),
                strength: None,
            },
            flake: Flake::default(),
            ..PaintJob::default()
        };
        // Missing colours take the game's defaults: base white, flake green.
        assert_eq!(
            icon_argument(&paint_job(None, metallic)).as_deref(),
            Some(
                "clr_0.mat color=FFFEFEFE><offset hshift=-256>\
                 <img src=/material/ui/clr_1.mat color=FFFE0000><offset hshift=-256>\
                 <img src=/material/ui/clr_2.mat color=FF00FE00><offset hshift=-256>\
                 <img src=/material/ui/clr_3.mat"
            )
        );
    }

    #[test]
    fn base_parts_use_fixed_tabs_and_accessories_get_pins() {
        assert_eq!(tab_of(AccessoryKind::Chassis, false), Tab::Chassis);
        assert_eq!(tab_of(AccessoryKind::PaintJob, false), Tab::Color);
        assert_eq!(tab_of(AccessoryKind::Interior, true), Tab::Interior);
        assert_eq!(
            tab_of(AccessoryKind::Addon, false),
            Tab::ExteriorAccessories
        );
        assert_eq!(tab_of(AccessoryKind::Addon, true), Tab::InteriorAccessories);
        assert_eq!(
            tab_of(AccessoryKind::Wheel, false),
            Tab::ExteriorAccessories
        );
        assert!(Tab::ExteriorAccessories.has_pins() && !Tab::Cabin.has_pins());
        for token in [
            "cabin",
            "chassis",
            "engine",
            "transmission",
            "color",
            "e_accessories",
            "interior",
            "i_accessories",
        ] {
            assert!(Tab::from_token(token).is_some(), "{token}");
        }
        assert_eq!(Tab::from_token("trailer_body"), None);
    }

    #[test]
    fn humanizes_slot_names() {
        assert_eq!(humanize("f_tire"), "Front Tire");
        assert_eq!(humanize("sideskirt"), "Sideskirt");
        assert_eq!(humanize("r_chs_cover"), "Rear Chs Cover");
    }
}
