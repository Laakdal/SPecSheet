//! The game's tonemap pass (`def.tonemap`), written clean-room from
//! `docs/app/shader-specs/tonemap.md` §4 (D13): the low-light tint, the
//! exposure, the optional colour correction and the three-segment tone curve
//! with its hue handling. It runs on the camera's HDR target in place of
//! Bevy's tonemapper. No eye adaptation or bloom yet: the exposure is a
//! fixed input, and every engine value is a raw input (§6), so findings
//! change data, not the shader.

use bevy::asset::embedded_asset;
use bevy::core_pipeline::fullscreen_material::{FullscreenMaterial, FullscreenMaterialPlugin};
use bevy::prelude::*;
use bevy::render::extract_component::ExtractComponent;
use bevy::render::render_resource::ShaderType;
use bevy::shader::ShaderRef;

const SHADER: &str = "embedded://specsheet_render/tonemap.wgsl";

pub struct GameTonemapPlugin;

impl Plugin for GameTonemapPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "tonemap.wgsl");
        app.add_plugins(FullscreenMaterialPlugin::<GameTonemap>::default());
    }
}

/// The tonemap inputs on a camera; mirrors `GameTonemap` in tonemap.wgsl.
/// The symbols are the spec's. [`Default`] is the spec's identity test
/// vector (§7): colours up to white pass unchanged.
#[derive(Component, ExtractComponent, Clone, Copy, Debug, PartialEq, ShaderType)]
pub struct GameTonemap {
    /// T0: x the exposure multiplier (`k = T0.x·E`), z the white level W,
    /// w the strength of the per-channel curve blend.
    pub t0: Vec4,
    /// T1…T5: the curve's segment coefficients, and in T5.zw its two
    /// breakpoints (§4.4).
    pub curve: [Vec4; 5],
    /// `low_intensity_factors` (a, b) in xy (§4.2).
    pub low_intensity: Vec4,
    pub low_intensity_color: Vec4,
    /// Balance offset in xyz, saturation in w; used when `color_correct`
    /// is not 0 (the `col` flavour, §4.3).
    pub color_correction: Vec4,
    /// E, the adapted exposure scale: fixed until eye adaptation (§3).
    pub exposure: f32,
    pub color_correct: u32,
}

impl Default for GameTonemap {
    fn default() -> Self {
        // Every segment: (α, β) = (1, 32), (γ, δ) = (0, 1), (μ, ν) = (1, −32).
        let ab = Vec2::new(1.0, 32.0);
        let gd = Vec2::new(0.0, 1.0);
        let mn = Vec2::new(1.0, -32.0);
        Self {
            t0: Vec4::new(1.0, 0.0, 1.0, 0.0),
            curve: [
                ab.extend(gd.x).extend(gd.y),
                mn.extend(ab.x).extend(ab.y),
                gd.extend(mn.x).extend(mn.y),
                ab.extend(gd.x).extend(gd.y),
                mn.extend(0.0).extend(0.0),
            ],
            // f = sat((Y − a)·b) is 1 for every Y ≥ 0: no tint.
            low_intensity: Vec4::new(-1.0, 1.0, 0.0, 0.0),
            low_intensity_color: Vec4::ZERO,
            color_correction: Vec4::new(0.0, 0.0, 0.0, 1.0),
            exposure: 1.0,
            color_correct: 0,
        }
    }
}

impl FullscreenMaterial for GameTonemap {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// §4.4 on the CPU, to check the coefficients the shader receives.
    fn curve(t: &GameTonemap, x: f32) -> f32 {
        let c = t.curve;
        let s = x.max(2f32.powi(-24)).log2();
        let (ab, gd, mn) = if s < c[4].z {
            (c[0].xy(), c[0].zw(), c[1].xy())
        } else if s <= c[4].w {
            (c[1].zw(), c[2].xy(), c[2].zw())
        } else {
            (c[3].xy(), c[3].zw(), c[4].xy())
        };
        let u = (ab.x * s + ab.y).max(2f32.powi(-24));
        let v = (gd.x + gd.y * u.log2()).exp2();
        (mn.x * v + mn.y).exp2()
    }

    #[test]
    fn the_default_is_the_specs_identity() {
        let t = GameTonemap::default();
        for x in [1e-6, 0.01, 0.18, 0.5, 1.0, 4.0, 300.0] {
            let y = curve(&t, x);
            assert!((y - x).abs() <= x * 1e-5, "F({x}) = {y}");
        }
        assert_eq!(t.t0.x * t.exposure, 1.0, "k");
        assert_eq!((t.t0.z, t.t0.w), (1.0, 0.0), "W and no per-channel blend");
        assert_eq!(t.color_correct, 0);
    }
}
