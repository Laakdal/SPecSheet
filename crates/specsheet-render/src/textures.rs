//! The textures the game materials bind beyond the `.mat`'s base texture:
//! paint masks, flake noise and reflection cubes. Each loads once in the
//! background and is bound to every material waiting for it. A texture that
//! is loading or failed leaves its binding empty and its flag clear, so the
//! material draws without it.
//!
//! `/material/environment/vehicle_reflection.tobj` names the engine's vehicle
//! reflection slot rather than only a file (F16,
//! `research/evidence/exposure-tonemap-data.txt` §4): the scene decides which
//! cube fills it ([`GameTextures::set_vehicle_slot`]).

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;
use bevy::render::renderer::RenderDevice;
use bevy::render::settings::WgpuFeatures;
use bevy::tasks::{futures::check_ready, AsyncComputeTaskPool, Task};
use specsheet_data::render::{load_cube_texture, load_texture, CubeTextureData, TextureData};
use specsheet_session::GameSession;

use super::difspec::DifSpecMaterial;
use super::truckpaint::{PaintBinding, TruckPaintMaterial};
use crate::assets;

/// Where a finished texture goes.
#[derive(Clone, Debug)]
pub enum Target {
    Paint(Handle<TruckPaintMaterial>, PaintBinding),
    /// A `dif.spec` material's reflection cube.
    DifSpec(Handle<DifSpecMaterial>),
}

impl Target {
    fn cube(&self) -> bool {
        matches!(
            self,
            Target::Paint(_, PaintBinding::Reflection) | Target::DifSpec(_)
        )
    }
}

enum Loaded {
    Flat(TextureData),
    Cube(CubeTextureData),
}

enum TextureState {
    Loading(Task<Result<Loaded, String>>),
    Ready(Handle<Image>),
    Failed,
}

/// The path that names the engine's vehicle reflection slot.
pub const VEHICLE_SLOT: &str = "/material/environment/vehicle_reflection.tobj";

/// Ours (approximation): the stand-in for the reflection cube the engine
/// renders of a `cube_reflection` scene (Hypothesis, F16): the garage
/// models' own flat grey cube. The engine never loads this file by name.
pub const SCENE_CUBE_STAND_IN: &str = "/material/environment/ui_vehicle_reflection.tobj";

/// The cube that fills the vehicle slot in a scene with these keys: its
/// `vehicle_reflection`, else for `cube_reflection` our stand-in for the
/// rendered scene cube, else `None` (the slot's own file).
pub fn vehicle_slot_cube(
    vehicle_reflection: Option<&str>,
    cube_reflection: bool,
) -> Option<String> {
    vehicle_reflection
        .map(str::to_owned)
        .or_else(|| cube_reflection.then(|| SCENE_CUBE_STAND_IN.to_owned()))
}

/// A material waiting for a texture.
struct Waiting {
    target: Target,
    /// The texture loading for it.
    path: String,
    /// Requested through the vehicle slot.
    slot: bool,
}

/// Textures by game path (and whether a cube), and the materials waiting
/// for them.
#[derive(Resource, Default)]
pub struct GameTextures {
    textures: HashMap<(String, bool), TextureState>,
    waiting: Vec<Waiting>,
    /// The cube filling the vehicle slot; `None`: the slot's own file.
    vehicle_slot: Option<String>,
    /// Every material that samples the vehicle slot, to rebind on a change.
    slot_users: Vec<Target>,
}

impl GameTextures {
    /// Fills the vehicle slot with `cube` (`None`: the slot's own file) and
    /// rebinds every material that samples it.
    pub fn set_vehicle_slot(&mut self, game: Option<&Arc<GameSession>>, cube: Option<String>) {
        if self.vehicle_slot == cube {
            return;
        }
        self.vehicle_slot = cube;
        // A pending load of the old cube must not land after the new one.
        self.waiting.retain(|w| !w.slot);
        for target in std::mem::take(&mut self.slot_users) {
            self.request(game, VEHICLE_SLOT, target);
        }
    }

    /// Binds `path` to `target` once it has loaded.
    pub fn request(&mut self, game: Option<&Arc<GameSession>>, path: &str, target: Target) {
        let slot = path == VEHICLE_SLOT;
        if slot {
            self.slot_users.push(target.clone());
        }
        let path = match &self.vehicle_slot {
            Some(cube) if slot => cube.clone(),
            _ => path.to_owned(),
        };
        let path = path.as_str();
        let cube = target.cube();
        if let Entry::Vacant(entry) = self.textures.entry((path.to_owned(), cube)) {
            let Some(game) = game.cloned() else {
                return;
            };
            let task_path = path.to_owned();
            let task = AsyncComputeTaskPool::get().spawn(async move {
                let loaded = if cube {
                    load_cube_texture(game.fs(), &task_path).map(Loaded::Cube)
                } else {
                    load_texture(game.fs(), &task_path).map(Loaded::Flat)
                };
                loaded.map_err(|e| e.to_string())
            });
            entry.insert(TextureState::Loading(task));
        }
        self.waiting.push(Waiting {
            target,
            path: path.to_owned(),
            slot,
        });
    }
}

/// Uploads finished textures and binds them to the materials waiting for
/// them.
pub fn finish_game_textures(
    mut textures: ResMut<GameTextures>,
    mut images: ResMut<Assets<Image>>,
    mut paints: ResMut<Assets<TruckPaintMaterial>>,
    mut dif_specs: ResMut<Assets<DifSpecMaterial>>,
    device: Option<Res<RenderDevice>>,
) {
    if textures.waiting.is_empty() {
        return;
    }
    let bc_supported =
        device.is_some_and(|d| d.features().contains(WgpuFeatures::TEXTURE_COMPRESSION_BC));
    let textures = &mut *textures;
    for ((path, _), state) in &mut textures.textures {
        let TextureState::Loading(task) = state else {
            continue;
        };
        let Some(result) = check_ready(task) else {
            continue;
        };
        let image = result.and_then(|loaded| match loaded {
            Loaded::Flat(data) => assets::texture_image(data, bc_supported),
            Loaded::Cube(data) => assets::cube_image(data, bc_supported),
        });
        *state = match image {
            Ok(image) => TextureState::Ready(images.add(image)),
            Err(error) => {
                warn!("material texture {path}: {error}");
                TextureState::Failed
            }
        };
    }
    textures.waiting.retain(|Waiting { target, path, .. }| {
        let image = match textures.textures.get(&(path.clone(), target.cube())) {
            Some(TextureState::Loading(_)) => return true,
            Some(TextureState::Ready(image)) => image.clone(),
            Some(TextureState::Failed) | None => return false,
        };
        match target {
            Target::Paint(handle, binding) => {
                if let Some(mut material) = paints.get_mut(handle) {
                    material.extension.bind(*binding, image);
                }
            }
            Target::DifSpec(handle) => {
                if let Some(mut material) = dif_specs.get_mut(handle) {
                    material.extension.bind_reflection(image);
                }
            }
        }
        false
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const GREY: &str = "/material/environment/ui_vehicle_reflection.tobj";

    /// Textures marked failed, so requests need no game to load them.
    fn textures(paths: &[&str]) -> GameTextures {
        let mut textures = GameTextures::default();
        for path in paths {
            textures
                .textures
                .insert(((*path).to_owned(), true), TextureState::Failed);
        }
        textures
    }

    fn waiting(textures: &GameTextures) -> Vec<&str> {
        textures.waiting.iter().map(|w| w.path.as_str()).collect()
    }

    #[test]
    fn a_scene_fills_the_slot_with_its_cube_or_our_stand_in() {
        let hangar = "/model/ui/hangar/hangar_cubemap.tobj";
        assert_eq!(
            vehicle_slot_cube(Some(hangar), true).as_deref(),
            Some(hangar)
        );
        assert_eq!(
            vehicle_slot_cube(None, true).as_deref(),
            Some(SCENE_CUBE_STAND_IN)
        );
        assert_eq!(vehicle_slot_cube(None, false), None);
    }

    #[test]
    fn the_vehicle_slot_follows_the_scene_and_other_cubes_stay() {
        let mut t = textures(&[VEHICLE_SLOT, GREY]);
        t.request(None, VEHICLE_SLOT, Target::DifSpec(Handle::default()));
        t.request(None, GREY, Target::DifSpec(Handle::default()));
        assert_eq!(waiting(&t), [VEHICLE_SLOT, GREY]);

        t.set_vehicle_slot(None, Some(GREY.to_owned()));
        assert_eq!(
            waiting(&t),
            [GREY, GREY],
            "the slot's user rebinds; the old pending load is dropped"
        );
        t.request(None, VEHICLE_SLOT, Target::DifSpec(Handle::default()));
        assert_eq!(
            waiting(&t),
            [GREY, GREY, GREY],
            "later users take the slot too"
        );

        t.set_vehicle_slot(None, None);
        assert_eq!(waiting(&t), [GREY, VEHICLE_SLOT, VEHICLE_SLOT]);
    }
}
