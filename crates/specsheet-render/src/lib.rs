//! Game materials on Bevy (D17): our own shaders, written clean-room from
//! the specs in `docs/app/shader-specs/` (D13), the game's lighting, and
//! [`assets`], which turns `specsheet-data` meshes and textures into
//! checked Bevy meshes, materials and images.

pub mod assets;
mod difspec;
mod lighting;
mod shadow;
mod textures;
mod tonemap;
mod truckpaint;

use std::sync::Arc;

use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
use specsheet_session::GameSession;

pub use difspec::{DifSpecMaterial, DifSpecMaterials, DifSpecSurface};
pub use lighting::{GameLight, GameLighting};
pub use shadow::{ShadowOnly, ShadowOnlyMaterial};
pub use textures::{vehicle_slot_cube, GameTextures};
pub use tonemap::GameTonemap;
pub use truckpaint::{Paint, PaintMaterials, PaintSurface, TruckPaintMaterial};

/// The game lighting, the clean-room materials, their textures and the
/// game's tonemap pass.
pub struct GameMaterialsPlugin;

impl Plugin for GameMaterialsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            lighting::GameLightingPlugin,
            truckpaint::TruckPaintPlugin,
            difspec::DifSpecPlugin,
            tonemap::GameTonemapPlugin,
            shadow::ShadowOnlyPlugin,
        ))
        .init_resource::<GameTextures>()
        .add_systems(Update, textures::finish_game_textures);
    }
}

/// What every game material is built from: the `.mat` stand-ins, the
/// texture loader and the scene's light.
pub struct MaterialContext<'a> {
    pub game: Option<&'a Arc<GameSession>>,
    pub standard: &'a Assets<StandardMaterial>,
    pub textures: &'a mut GameTextures,
    pub light: &'a Handle<ShaderBuffer>,
}
