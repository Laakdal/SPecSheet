//! The sRGB transfer function. Game colours in `.sii` and `.mat` files are
//! linear; textures flagged `_SRGB` and UI markup colours are sRGB-encoded.

/// sRGB-encoded channel in `[0, 1]` to linear.
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear channel to sRGB-encoded, clamped to `[0, 1]` first.
pub fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_directions_invert_each_other() {
        assert!((srgb_to_linear(0.5) - 0.214_041).abs() < 1e-5);
        assert_eq!(srgb_to_linear(0.0), 0.0);
        for i in 0..=20 {
            let c = i as f32 / 20.0;
            assert!((srgb_to_linear(linear_to_srgb(c)) - c).abs() < 1e-5, "{c}");
        }
        assert!((linear_to_srgb(2.0) - 1.0).abs() < 1e-6, "clamped");
    }
}
