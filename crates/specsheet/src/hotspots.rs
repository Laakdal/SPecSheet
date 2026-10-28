//! Pins over the 3D view, drawn the game's way (finding F5,
//! `research/evidence/pin-attributes.txt`):
//! * each pin fades with the angle between its facing and the camera's view
//!   axis ([`specsheet_data::pins::opacity`]);
//! * a wheel's hub, cover and nut pins show only while that wheel's disc
//!   pin is open;
//! * nearby pins never merge and there is no depth test against the truck.

use bevy::math::Vec3;

/// How opaque a pin is: `facing` is the pin's facing in world space (`None`
/// for interior pins, which always show), `view_axis` points from the scene
/// towards the camera.
pub fn opacity(facing: Option<Vec3>, view_axis: Vec3) -> f32 {
    facing.map_or(1.0, |f| {
        specsheet_data::pins::opacity(f.to_array(), view_axis.to_array())
    })
}

/// Whether a pin linked to `link` shows while `open` is the open pin.
pub fn shown(link: Option<usize>, open: Option<usize>) -> bool {
    link.is_none() || link == open
}

/// The open pin after clicking pin `clicked`: a pin that expands opens,
/// a linked pin keeps its target open, any other pin closes it (the game's
/// pin select, F5).
pub fn open_after_click(
    clicked: usize,
    expands: bool,
    link: Option<usize>,
    open: Option<usize>,
) -> Option<usize> {
    if expands {
        Some(clicked)
    } else if link.is_some() {
        open
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pins_facing_away_fade_out_and_interior_pins_always_show() {
        let view_axis = Vec3::NEG_X;
        assert_eq!(opacity(Some(Vec3::NEG_X), view_axis), 1.0);
        assert_eq!(opacity(Some(Vec3::X), view_axis), 0.0);
        assert_eq!(opacity(None, view_axis), 1.0);
    }

    #[test]
    fn a_disc_pin_opens_its_hub_cover_and_nut_pins() {
        // Pin 3 is a disc; pins 4 and 5 link to it; pin 0 is a mirror.
        assert!(!shown(Some(3), None));
        let open = open_after_click(3, true, None, None);
        assert_eq!(open, Some(3));
        assert!(shown(Some(3), open) && !shown(Some(7), open));
        assert_eq!(open_after_click(4, false, Some(3), open), Some(3));
        assert_eq!(open_after_click(0, false, None, open), None);
        assert!(shown(None, None), "unlinked pins always show");
    }
}
