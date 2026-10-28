//! The game's scene lighting for the clean-room materials (D13): the sun,
//! ambient and reflection-visibility inputs of `game_lighting.wgsl`
//! (dif-spec.md §3.3, truckpaint-lighting.md §5). One storage buffer holds
//! them, and every game material binds that same buffer, so a material
//! created after a scene change still sees the scene's light.

use bevy::prelude::*;
use bevy::render::render_resource::ShaderType;
use bevy::render::storage::ShaderBuffer;
use bevy::shader::load_shader_library;

/// The game's colours are linear HDR, and how its eye adaptation and tone
/// mapping turn them into pixels is Unknown (F16; REA on the tonemap
/// parameters is queued). Ours, without a reference: they go straight into
/// Bevy's tonemapper.
const OUTPUT_SCALE: f32 = 1.0;

pub struct GameLightingPlugin;

impl Plugin for GameLightingPlugin {
    fn build(&self, app: &mut App) {
        load_shader_library!(app, "game_lighting.wgsl");
    }

    fn finish(&self, app: &mut App) {
        app.init_resource::<GameLighting>();
    }
}

/// Mirrors `GameLight` in game_lighting.wgsl.
#[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
pub struct GameLight {
    /// L: the unit vector towards the sun.
    to_sun: Vec4,
    sun_diffuse: Vec4,
    sun_specular: Vec4,
    /// Camb; w: Aenv, the reflection visibility.
    ambient: Vec4,
    /// Cadd; w: [`OUTPUT_SCALE`].
    additional_ambient: Vec4,
}

impl GameLight {
    /// From a scene's `light_direction` (the direction the light travels),
    /// `light_diffuse`, `light_specular` (the diffuse colour when absent),
    /// `light_ambient` and `light_environment` (Aenv, Hypothesis). The
    /// scene has no additional ambient colour, so Cadd is 0.
    pub fn new(
        travel: Vec3,
        diffuse: [f32; 3],
        specular: Option<[f32; 3]>,
        ambient: [f32; 3],
        environment: f32,
    ) -> Self {
        let to_sun = (-travel).try_normalize().unwrap_or(Vec3::Y);
        Self {
            to_sun: to_sun.extend(0.0),
            sun_diffuse: Vec3::from(diffuse).extend(0.0),
            sun_specular: Vec3::from(specular.unwrap_or(diffuse)).extend(0.0),
            ambient: Vec3::from(ambient).extend(environment),
            additional_ambient: Vec3::ZERO.extend(OUTPUT_SCALE),
        }
    }
}

/// The buffer every game material binds as its light.
#[derive(Resource)]
pub struct GameLighting {
    pub buffer: Handle<ShaderBuffer>,
    light: Option<GameLight>,
}

impl FromWorld for GameLighting {
    fn from_world(world: &mut World) -> Self {
        let light = GameLight::new(Vec3::NEG_Y, [1.0; 3], None, [0.3; 3], 1.0);
        let buffer = world
            .resource_mut::<Assets<ShaderBuffer>>()
            .add(ShaderBuffer::from(light));
        Self {
            buffer,
            light: Some(light),
        }
    }
}

impl GameLighting {
    /// Writes `light` into the shared buffer when it changed.
    pub fn set(&mut self, buffers: &mut Assets<ShaderBuffer>, light: GameLight) {
        if self.light == Some(light) {
            return;
        }
        if let Some(mut buffer) = buffers.get_mut(&self.buffer) {
            buffer.set_data(light);
            self.light = Some(light);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sun_points_against_its_travel_and_specular_defaults_to_diffuse() {
        let light = GameLight::new(
            Vec3::new(0.0, -2.0, 0.0),
            [1.0, 0.5, 0.25],
            None,
            [0.1; 3],
            2.0,
        );
        assert_eq!(light.to_sun, Vec4::new(0.0, 1.0, 0.0, 0.0));
        assert_eq!(light.sun_specular, light.sun_diffuse);
        assert_eq!(light.ambient.w, 2.0);
        assert_eq!(light.additional_ambient.w, OUTPUT_SCALE);
        let up = GameLight::new(Vec3::ZERO, [1.0; 3], Some([0.5; 3]), [0.0; 3], 0.0);
        assert_eq!(
            up.to_sun,
            Vec4::Y,
            "a zero direction falls back to overhead"
        );
        assert_eq!(up.sun_specular, Vec4::new(0.5, 0.5, 0.5, 0.0));
    }
}
