//! The textures the game materials bind beyond the `.mat`'s base texture:
//! paint masks, flake noise and reflection cubes. Each loads once in the
//! background and is bound to every material waiting for it. A texture that
//! is loading or failed leaves its binding empty and its flag clear, so the
//! material draws without it.

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

/// Textures by game path (and whether a cube), and the materials waiting
/// for them.
#[derive(Resource, Default)]
pub struct GameTextures {
    textures: HashMap<(String, bool), TextureState>,
    waiting: Vec<(Target, String)>,
}

impl GameTextures {
    /// Binds `path` to `target` once it has loaded.
    pub fn request(&mut self, game: Option<&Arc<GameSession>>, path: &str, target: Target) {
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
        self.waiting.push((target, path.to_owned()));
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
    textures.waiting.retain(|(target, path)| {
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
