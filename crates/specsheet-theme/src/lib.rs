//! Design tokens for SPecSheet's own screens (design system D16,
//! `docs/app/design-system.md`): colour roles in a dark and a light
//! appearance, the type scale and spacing. Front ends map these onto their
//! toolkit once; no screen hard-codes a colour or size.
//!
//! Not for the tuning screen: that is the game's own UI, drawn from its
//! layout files, and is never restyled with these tokens.

#![warn(missing_docs)]

/// An sRGB colour, 8 bits per channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// The colour roles of one appearance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    /// Background.
    pub page: Rgb,
    /// Hover, fields, segmented track.
    pub plate_soft: Rgb,
    /// Pressed, switch off.
    pub plate: Rgb,
    /// Dialogs and menus above the page.
    pub surface: Rgb,
    /// 1 px row dividers.
    pub rule: Rgb,
    /// Text and selection.
    pub ink: Rgb,
    /// Secondary text and values.
    pub ink_soft: Rgb,
    /// Labels, descriptions, mono indices.
    pub muted: Rgb,
    /// The single main action, focus, links, progress: truck-signal amber.
    pub accent: Rgb,
    /// Text on `accent`.
    pub on_accent: Rgb,
    /// Errors and destructive actions.
    pub danger: Rgb,
    /// Completed and compatible states.
    pub success: Rgb,
    /// Pin colour over the 3D view for exterior accessories.
    pub pin_base: Rgb,
    /// Pin colour for wheel groups.
    pub pin_wheel: Rgb,
    /// Pin colour for interior accessories.
    pub pin_interior: Rgb,
}

/// The dark appearance (the app's default).
pub const DARK: Palette = Palette {
    page: Rgb(0x11, 0x13, 0x17),
    plate_soft: Rgb(0x1a, 0x1d, 0x23),
    plate: Rgb(0x2e, 0x33, 0x3d),
    surface: Rgb(0x22, 0x26, 0x2e),
    rule: Rgb(0x2e, 0x33, 0x3d),
    ink: Rgb(0xe8, 0xea, 0xee),
    ink_soft: Rgb(0xbf, 0xc4, 0xcc),
    muted: Rgb(0x8d, 0x94, 0xa1),
    accent: Rgb(0xf5, 0xa6, 0x23),
    on_accent: Rgb(0x11, 0x13, 0x17),
    danger: Rgb(0xef, 0x6b, 0x6b),
    success: Rgb(0x8b, 0xd1, 0x7c),
    pin_base: Rgb(0x8b, 0xd1, 0x7c),
    pin_wheel: Rgb(0x5f, 0xb3, 0xf9),
    pin_interior: Rgb(0xc9, 0xa2, 0xf5),
};

/// The light appearance.
pub const LIGHT: Palette = Palette {
    page: Rgb(0xfa, 0xf8, 0xf5),
    plate_soft: Rgb(0xf0, 0xed, 0xe8),
    plate: Rgb(0xe2, 0xde, 0xd7),
    surface: Rgb(0xff, 0xff, 0xff),
    rule: Rgb(0xe2, 0xde, 0xd7),
    ink: Rgb(0x1a, 0x1a, 0x1c),
    ink_soft: Rgb(0x3d, 0x3d, 0x42),
    muted: Rgb(0x5f, 0x5e, 0x63),
    accent: Rgb(0x9a, 0x5a, 0x00),
    on_accent: Rgb(0xff, 0xff, 0xff),
    danger: Rgb(0xb4, 0x23, 0x18),
    success: Rgb(0x2e, 0x7d, 0x32),
    pin_base: Rgb(0x2e, 0x7d, 0x32),
    pin_wheel: Rgb(0x15, 0x65, 0xc0),
    pin_interior: Rgb(0x7b, 0x3f, 0xc4),
};

/// One step of the type scale, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypeStep {
    /// Font size.
    pub size: f32,
    /// Line height.
    pub line_height: f32,
    /// Letter spacing in em.
    pub tracking: f32,
    /// Font weight, 100 to 900.
    pub weight: u16,
}

const fn step(size: f32, line_height: f32, tracking: f32, weight: u16) -> TypeStep {
    TypeStep {
        size,
        line_height,
        tracking,
        weight,
    }
}

/// Tiny indices and badges.
pub const MICRO: TypeStep = step(11.0, 14.0, 0.02, 500);
/// Captions and secondary notes.
pub const CAPTION: TypeStep = step(12.0, 16.0, 0.01, 400);
/// Field and list labels.
pub const LABEL: TypeStep = step(13.0, 18.0, 0.0, 500);
/// Default control text.
pub const UI: TypeStep = step(14.0, 20.0, 0.0, 400);
/// Running text.
pub const BODY: TypeStep = step(15.0, 22.0, 0.0, 400);
/// Section headings.
pub const HEADING: TypeStep = step(20.0, 26.0, -0.01, 600);
/// Screen titles.
pub const TITLE: TypeStep = step(28.0, 32.0, -0.02, 600);
/// Large display numbers and hero text.
pub const DISPLAY: TypeStep = step(44.0, 48.0, -0.03, 700);
/// The faint "SPecSheet" signature behind the home screen.
pub const WORDMARK: TypeStep = step(160.0, 160.0, -0.04, 800);

/// The spacing scale, in logical pixels.
pub const SPACE: [f32; 9] = [4.0, 8.0, 12.0, 16.0, 24.0, 32.0, 48.0, 64.0, 96.0];
/// Page margin.
pub const PAGE_MARGIN: f32 = 64.0;
/// Page margin when the window is narrow.
pub const PAGE_MARGIN_NARROW: f32 = 16.0;
/// Corner radius of panels and controls.
pub const RADIUS: f32 = 8.0;
/// Diameter of a pin dot over the 3D view.
pub const PIN_SIZE: f32 = 16.0;

/// WCAG relative luminance of an sRGB colour.
pub fn luminance(c: Rgb) -> f64 {
    let channel = |v: u8| {
        let v = f64::from(v) / 255.0;
        if v <= 0.040_45 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(c.0) + 0.7152 * channel(c.1) + 0.0722 * channel(c.2)
}

/// WCAG contrast ratio between two colours (1 to 21).
pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every text role meets WCAG AA (4.5:1) on every background it is drawn
    /// on, in both appearances. A palette that fails this doesn't ship.
    #[test]
    fn text_roles_meet_wcag_aa_on_every_background() {
        for (name, p) in [("dark", DARK), ("light", LIGHT)] {
            for (text, t) in [("ink", p.ink), ("ink_soft", p.ink_soft), ("muted", p.muted)] {
                for (bg, b) in [
                    ("page", p.page),
                    ("plate_soft", p.plate_soft),
                    ("surface", p.surface),
                ] {
                    let ratio = contrast(t, b);
                    assert!(ratio >= 4.5, "{name}: {text} on {bg} is {ratio:.2}:1");
                }
            }
            let ratio = contrast(p.on_accent, p.accent);
            assert!(ratio >= 4.5, "{name}: on_accent on accent is {ratio:.2}:1");
        }
    }

    #[test]
    fn contrast_is_symmetric_and_bounded() {
        let (black, white) = (Rgb(0, 0, 0), Rgb(255, 255, 255));
        assert!((contrast(black, white) - 21.0).abs() < 1e-9);
        assert_eq!(contrast(white, black), contrast(black, white));
        assert!((contrast(white, white) - 1.0).abs() < 1e-9);
    }
}
