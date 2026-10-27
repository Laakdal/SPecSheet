//! Presentation-neutral views of a [`TruckSession`]: display names, which
//! slots get a marker, and plain-language verdicts. Front ends render these
//! as they are and hold no tuning rules (see `docs/ARCHITECTURE.md`).

use std::collections::HashSet;

use specsheet_core::{AccessoryId, AccessoryKind, Incompatibility, SlotId, TruckId};

use crate::assembly::Drive;
use crate::session::{GameSession, SlotOption, TruckSession};

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
    /// Truck-space anchor (x right, y up, -z forward).
    pub position: [f32; 3],
    pub category: HotspotCategory,
    /// Display name of the selected accessory.
    pub selected: Option<String>,
    pub option_count: usize,
    /// Nothing is placed in the slot; the anchor sits on bare locators.
    pub empty: bool,
    /// Hotspots of one group (e.g. a wheel axle's parts) share a position
    /// and form one marker.
    pub group: Option<String>,
}

/// A slot without a marker (paint job, engine, interior-only parts...).
#[derive(Clone, Debug, PartialEq)]
pub struct SlotEntry {
    pub slot: SlotId,
    pub label: String,
    pub selected: Option<String>,
    pub option_count: usize,
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
    pub warnings: Vec<String>,
}

/// One option of a slot with the rules' verdict.
#[derive(Clone, Debug, PartialEq)]
pub struct OptionView {
    pub id: AccessoryId,
    pub name: String,
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
    let hotspots: Vec<Hotspot> = assembly
        .anchors
        .iter()
        .filter_map(|anchor| {
            let mut options = catalog.in_slot(&anchor.slot);
            let kind = options.next()?.kind;
            let option_count = 1 + options.count();
            // Only slots with a visible choice get a marker.
            if option_count < 2 && kind != AccessoryKind::Addon {
                return None;
            }
            Some(Hotspot {
                slot: anchor.slot.clone(),
                label: slot_label(game, &anchor.slot),
                position: anchor.position,
                category: if anchor.interior {
                    HotspotCategory::Interior
                } else {
                    category(kind)
                },
                selected: selected_name(game, session, &anchor.slot),
                option_count,
                empty: anchor.empty,
                group: anchor.group.clone(),
            })
        })
        .collect();
    let marked: HashSet<&SlotId> = hotspots.iter().map(|h| &h.slot).collect();
    let general = catalog
        .slots()
        .into_iter()
        .filter(|slot| !marked.contains(slot))
        .map(|slot| SlotEntry {
            label: slot_label(game, &slot),
            selected: selected_name(game, session, &slot),
            option_count: catalog.in_slot(&slot).count(),
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
    use super::humanize;

    #[test]
    fn humanizes_slot_names() {
        assert_eq!(humanize("f_tire"), "Front Tire");
        assert_eq!(humanize("sideskirt"), "Sideskirt");
        assert_eq!(humanize("r_chs_cover"), "Rear Chs Cover");
    }
}
