//! The accessory catalog of one truck model, as loaded from game data.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::hookup::Hookup;
use crate::paint::PaintJob;
use specsheet_core::{AccessoryId, SlotId, TruckId};

/// What kind of part an accessory is. Determines how it is assembled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessoryKind {
    /// Base model the rest of the truck is built on.
    Chassis,
    /// Base model mounted on the chassis.
    Cabin,
    /// A model attached at locators of the base models.
    Addon,
    /// A wheel part (tyre, rim, hub, nuts, cover) attached at the chassis
    /// wheel locators; shared by all trucks.
    Wheel,
    /// The cab interior (`interior` slot).
    Interior,
    /// The engine (non-visual).
    Engine,
    /// The gearbox (non-visual).
    Transmission,
    /// The headlight set.
    HeadLights,
    /// A paint job (`paint_job` slot).
    PaintJob,
    /// Anything else (non-visual or not yet supported).
    Other,
}

impl AccessoryKind {
    /// Kinds whose model forms the base the addons attach to.
    pub fn is_base(self) -> bool {
        matches!(self, Self::Chassis | Self::Cabin)
    }
}

/// Role of a wheel part in the wheel assembly (finding F9 in
/// `docs/REVERSE_ENGINEERING.md`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WheelPart {
    /// A tyre (`accessory_wheel_data`; also the legacy complete wheels).
    Tire,
    /// A part mounted on the hub face (`accessory_rim_data`: disc, rim, hub,
    /// nuts, cover).
    Rim {
        /// `tire_shift`: lateral offset in metres of the tyre on this rim,
        /// positive towards the truck's centre line. `None` when the
        /// definition has none.
        tire_shift: Option<f32>,
    },
}

/// Model references of an accessory. Paths are absolute game paths.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelRef {
    /// The model seen from outside.
    pub exterior: Option<String>,
    /// Right-hand-drive replacement of `exterior`.
    pub exterior_uk: Option<String>,
    /// The look of `exterior`.
    pub look: Option<String>,
    /// The variant of `exterior`.
    pub variant: Option<String>,
    /// The variant for right-hand drive.
    pub variant_uk: Option<String>,
    /// Interiors: the full cockpit seen from the driver's seat (`model`,
    /// `look`, `variant`); `exterior` is the light exterior-view model.
    #[serde(default)]
    pub cockpit: Option<String>,
    /// The look of `cockpit`.
    #[serde(default)]
    pub cockpit_look: Option<String>,
    /// The variant of `cockpit`.
    #[serde(default)]
    pub cockpit_variant: Option<String>,
    /// Right-hand-drive replacement of `cockpit` (`interior_model_uk`).
    #[serde(default)]
    pub cockpit_uk: Option<String>,
    /// Cabins: the side windows' glass, a model of its own in the cabin's
    /// space (`windows_model`; animated open by `left/right_window_anim`).
    #[serde(default)]
    pub windows: Option<String>,
    /// Cabins: the wipers (`wiper_model`), in the cabin's space.
    #[serde(default)]
    pub wipers: Option<String>,
    /// Right-hand-drive replacement of `wipers` (`wiper_model_uk`).
    #[serde(default)]
    pub wipers_uk: Option<String>,
    /// Chassis: the shadow quad under the truck on the tuning screen
    /// (`ui_shadow`), in the chassis's space.
    #[serde(default)]
    pub ui_shadow: Option<String>,
}

/// One purchasable/selectable part.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Accessory {
    /// The unit name, e.g. `chrome.scania.s_2016.s_mirror`.
    pub id: AccessoryId,
    /// The slot it fills.
    pub slot: SlotId,
    /// What kind of part it is.
    pub kind: AccessoryKind,
    /// Definition file, e.g. `/def/vehicle/truck/x/accessory/s_mirror/stock.sii`.
    pub def_path: String,
    /// Localisation key or literal display name (`@@key@@` form).
    pub name: String,
    /// The list icon (a UI material name).
    pub icon: Option<String>,
    /// Price in the game's currency.
    pub price: Option<i64>,
    /// Player level that unlocks it.
    pub unlock_level: Option<i64>,
    /// The models it draws.
    pub model: ModelRef,
    /// The accessory is only allowed when at least one of these is selected.
    pub suitable_for: Vec<AccessoryId>,
    /// The accessory is not allowed while any of these is selected.
    pub conflict_with: Vec<AccessoryId>,
    /// Definitions applied alongside this accessory when it is selected.
    pub defaults: Vec<String>,
    /// Slots that must be filled while this accessory is selected.
    pub requires: Vec<SlotId>,
    /// Default body colour of a paint job (linear RGB).
    pub base_color: Option<[f32; 3]>,
    /// Wheel parts only: how the part takes part in the wheel assembly.
    #[serde(default)]
    pub wheel: Option<WheelPart>,
    /// An interior for left-hand traffic (`left_hand_traffic: true`), so
    /// right-hand drive. Choosing one sets the truck's drive side (F10).
    #[serde(default)]
    pub left_hand_traffic: bool,
    /// The same interior for the other drive side (F10).
    #[serde(default)]
    pub drive_twin: Option<AccessoryId>,
    /// Paint jobs only: masks, colours, flakes and per-part overrides
    /// (F15). `base_color` above stays the paint job's base colour too.
    #[serde(default)]
    pub paint: Option<Box<PaintJob>>,
}

impl Accessory {
    /// Addons with an exterior model are placed at base-model locators, so
    /// their slot must exist on the current chassis/cabin.
    pub fn needs_exterior_locator(&self) -> bool {
        self.kind == AccessoryKind::Addon && self.model.exterior.is_some()
    }
}

/// Every accessory of one truck plus the truck-level rules.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TruckCatalog {
    /// The truck these accessories belong to.
    pub truck: TruckId,
    /// Localisation keys of brand and series (`info[]` of the truck data).
    pub info: Vec<String>,
    /// Slots every configuration must fill.
    pub required_slots: Vec<SlotId>,
    /// Definitions used when a required slot would otherwise stay empty.
    pub fallbacks: Vec<String>,
    accessories: BTreeMap<AccessoryId, Accessory>,
    by_path: HashMap<String, AccessoryId>,
    /// Hookups that can be mounted on this truck's accessories.
    #[serde(default)]
    hookups: BTreeMap<AccessoryId, Hookup>,
}

impl TruckCatalog {
    /// A catalog of `accessories`, indexed by id and by definition path.
    pub fn new(
        truck: TruckId,
        info: Vec<String>,
        required_slots: Vec<SlotId>,
        fallbacks: Vec<String>,
        accessories: impl IntoIterator<Item = Accessory>,
    ) -> Self {
        let mut catalog = Self {
            truck,
            info,
            required_slots,
            fallbacks,
            accessories: BTreeMap::new(),
            by_path: HashMap::new(),
            hookups: BTreeMap::new(),
        };
        for accessory in accessories {
            catalog.by_path.insert(
                normalize_def_path(&accessory.def_path),
                accessory.id.clone(),
            );
            catalog.accessories.insert(accessory.id.clone(), accessory);
        }
        catalog
    }

    /// Adds the hookups that can be mounted on this truck's accessories.
    pub fn with_hookups(mut self, hookups: impl IntoIterator<Item = Hookup>) -> Self {
        self.hookups
            .extend(hookups.into_iter().map(|h| (h.id.clone(), h)));
        self
    }

    /// The hookup `id`, if it fits this truck.
    pub fn hookup(&self, id: &AccessoryId) -> Option<&Hookup> {
        self.hookups.get(id)
    }

    /// Hookups that fit on the accessory in `host`, in id order.
    pub fn hookups_for<'a>(&'a self, host: &SlotId) -> impl Iterator<Item = &'a Hookup> + 'a {
        let host = host.clone();
        self.hookups.values().filter(move |h| h.fits(&host))
    }

    /// The accessory `id`.
    pub fn accessory(&self, id: &AccessoryId) -> Option<&Accessory> {
        self.accessories.get(id)
    }

    /// Every accessory, in id order.
    pub fn accessories(&self) -> impl Iterator<Item = &Accessory> {
        self.accessories.values()
    }

    /// Resolves a definition path (as used by `defaults[]`, `fallback[]`).
    pub fn by_def_path(&self, path: &str) -> Option<&Accessory> {
        self.by_path
            .get(&normalize_def_path(path))
            .and_then(|id| self.accessories.get(id))
    }

    /// Accessories that go into `slot`, in id order.
    pub fn in_slot<'a>(&'a self, slot: &SlotId) -> impl Iterator<Item = &'a Accessory> + 'a {
        let slot = slot.clone();
        self.accessories.values().filter(move |a| a.slot == slot)
    }

    /// Every slot that has at least one accessory, sorted.
    pub fn slots(&self) -> Vec<SlotId> {
        let mut slots: Vec<SlotId> = self.accessories.values().map(|a| a.slot.clone()).collect();
        slots.sort();
        slots.dedup();
        slots
    }
}

/// Definition paths are compared without a leading slash and case-insensitively.
fn normalize_def_path(path: &str) -> String {
    path.trim_start_matches('/').to_ascii_lowercase()
}
