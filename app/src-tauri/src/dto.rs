//! Serializable view types sent to the frontend. Mirrored by
//! `app/ui/src/api.ts`; keep both in sync.

use game_data::assembly::{Drive, Placement};
use game_data::session::{GameSession, SlotOption, TruckSession};
use serde::Serialize;
use tuning_core::{AccessoryKind, Incompatibility, SlotId};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameInfo {
    pub root: String,
    pub trucks: Vec<TruckEntry>,
    pub warnings: Vec<String>,
}

/// A tuning scene from `/def/desktop/config*.sii`; unset fields mean the
/// definition does not specify them and the viewer keeps its defaults.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneDto {
    pub id: String,
    pub name: String,
    pub model: Option<SceneModelDto>,
    pub camera: Option<CameraDto>,
    pub light_direction: Option<[f32; 3]>,
    pub light_ambient: Option<[f32; 3]>,
    pub light_diffuse: Option<[f32; 3]>,
    pub light_environment: Option<f32>,
    pub fog_color: Option<[f32; 3]>,
    pub fog_density: Option<f32>,
    pub truck_position: [f32; 3],
    /// Euler angles in degrees.
    pub truck_rotation: [f32; 3],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneModelDto {
    pub path: String,
    pub look: Option<String>,
    pub variant: Option<String>,
    pub position: [f32; 3],
    pub parts: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraDto {
    pub position: [f32; 3],
    pub aim: [f32; 3],
    pub fov: f32,
}

impl SceneDto {
    pub fn build(game: &GameSession, scene: game_data::scene::Scene) -> Self {
        Self {
            name: game.localization().resolve(&scene.name),
            id: scene.path,
            model: scene.model.map(|m| SceneModelDto {
                path: m.path,
                look: m.look,
                variant: m.variant,
                position: m.position,
                parts: m.parts,
            }),
            camera: scene.camera.map(|c| CameraDto {
                position: c.position,
                aim: c.aim,
                fov: c.fov,
            }),
            light_direction: scene.light_direction,
            light_ambient: scene.light_ambient,
            light_diffuse: scene.light_diffuse,
            light_environment: scene.light_environment,
            fog_color: scene.fog_color,
            fog_density: scene.fog_density,
            truck_position: scene.truck_position,
            truck_rotation: scene.truck_rotation,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TruckEntry {
    pub id: String,
    pub name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TruckView {
    pub truck: String,
    pub name: String,
    pub right_hand_drive: bool,
    pub placements: Vec<PlacementDto>,
    pub hotspots: Vec<HotspotDto>,
    /// Slots without a marker (paint job, engine, interior-only parts...),
    /// listed in the sidebar's "General" group.
    pub slots: Vec<SlotEntryDto>,
    pub paint_color: Option<[f32; 3]>,
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacementDto {
    pub slot: String,
    pub accessory: String,
    pub model: String,
    pub look: Option<String>,
    pub variant: Option<String>,
    pub translation: [f32; 3],
    /// Quaternion `(x, y, z, w)`.
    pub rotation: [f32; 4],
    pub scale: f32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotspotDto {
    pub slot: String,
    pub label: String,
    /// Truck-space anchor (x right, y up, -z forward).
    pub position: [f32; 3],
    pub category: &'static str,
    pub selected: Option<String>,
    pub option_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotEntryDto {
    pub slot: String,
    pub label: String,
    pub selected: Option<String>,
    pub option_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotOptions {
    pub slot: String,
    pub label: String,
    pub options: Vec<OptionDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionDto {
    pub id: String,
    pub name: String,
    pub price: Option<i64>,
    pub unlock_level: Option<i64>,
    pub selected: bool,
    pub compatible: bool,
    /// Why the option is not compatible, in plain words.
    pub reason: Option<String>,
}

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

impl TruckView {
    pub fn build(game: &GameSession, session: &TruckSession) -> Self {
        let catalog = session.catalog();
        let assembly = session.assembly();
        let hotspots = assembly
            .anchors
            .iter()
            .filter_map(|anchor| {
                let options: Vec<_> = catalog.in_slot(&anchor.slot).collect();
                // Only slots with a visible choice get a marker.
                let kind = options.first()?.kind;
                if options.len() < 2 && kind != AccessoryKind::Addon {
                    return None;
                }
                let selected = session
                    .config()
                    .selected(&anchor.slot)
                    .and_then(|id| catalog.accessory(id))
                    .map(|a| game.localization().resolve(&a.name));
                Some(HotspotDto {
                    slot: anchor.slot.to_string(),
                    label: slot_label(game, &anchor.slot),
                    position: anchor.position,
                    category: if anchor.interior {
                        "interior"
                    } else {
                        category(kind)
                    },
                    selected,
                    option_count: options.len(),
                })
            })
            .collect::<Vec<HotspotDto>>();
        let marked: std::collections::HashSet<&str> =
            hotspots.iter().map(|h| h.slot.as_str()).collect();
        let slots = catalog
            .slots()
            .into_iter()
            .filter(|slot| !marked.contains(slot.as_str()))
            .map(|slot| SlotEntryDto {
                label: slot_label(game, &slot),
                selected: session
                    .config()
                    .selected(&slot)
                    .and_then(|id| catalog.accessory(id))
                    .map(|a| game.localization().resolve(&a.name)),
                option_count: catalog.in_slot(&slot).count(),
                slot: slot.to_string(),
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
        Self {
            truck: catalog.truck.to_string(),
            name: truck_name(game, &catalog.info, catalog.truck.as_str()),
            right_hand_drive: session.drive() == Drive::Right,
            placements: assembly.placements.iter().map(PlacementDto::from).collect(),
            hotspots,
            slots,
            paint_color: session.config().paint_color(catalog),
            warnings,
        }
    }
}

fn category(kind: AccessoryKind) -> &'static str {
    match kind {
        AccessoryKind::Chassis | AccessoryKind::Cabin => "base",
        AccessoryKind::Wheel => "wheel",
        _ => "addon",
    }
}

impl From<&Placement> for PlacementDto {
    fn from(p: &Placement) -> Self {
        Self {
            slot: p.slot.to_string(),
            accessory: p.accessory.to_string(),
            model: p.model.clone(),
            look: p.look.clone(),
            variant: p.variant.clone(),
            translation: p.transform.translation,
            rotation: p.transform.rotation,
            scale: p.transform.scale,
        }
    }
}

impl OptionDto {
    pub fn build(game: &GameSession, option: &SlotOption<'_>) -> Self {
        Self {
            id: option.accessory.id.to_string(),
            name: game.localization().resolve(&option.accessory.name),
            price: option.accessory.price,
            unlock_level: option.accessory.unlock_level,
            selected: option.selected,
            compatible: option.verdict.is_ok(),
            reason: option.verdict.as_ref().err().map(|r| describe(game, r)),
        }
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
