//! Paint jobs: what an `accessory_paint_job_data` unit and its per-part
//! `simple_paint_job_data` units describe (finding F15 in
//! `docs/REVERSE_ENGINEERING.md`, format in `docs/formats/paint.md`).
//!
//! Colours are RGB triples in `[0, 1]` as the game data writes them; the
//! catalog treats them as linear, like [`crate::Accessory::base_color`].

use serde::{Deserialize, Serialize};

use crate::ids::{AccessoryId, SlotId};

/// Flake noise texture used when a paint job names none (Hypothesis: the
/// executable holds this path, and 1,063 of the 2,488 units that write
/// `flake_noise` write exactly this one).
pub const DEFAULT_FLAKE_NOISE: &str = "/material/custom/flake_noise.tobj";

/// The game's paint-parameter defaults for values a paint job leaves out
/// (F17, `research/evidence/rea-paint-apply.txt`): Confirmed for per-part
/// units, a strong Hypothesis for paint-job attributes (`docs/formats/paint.md`).
pub const DEFAULT_BASE_COLOR: [f32; 3] = [1.0, 1.0, 1.0];
pub const DEFAULT_MASK_COLORS: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
pub const DEFAULT_FLIP_COLOR: [f32; 3] = [1.0, 0.0, 0.0];
pub const DEFAULT_FLIP_STRENGTH: f32 = 0.27;
pub const DEFAULT_FLAKE_COLOR: [f32; 3] = [0.0, 1.0, 0.0];
pub const DEFAULT_FLAKE_DENSITY: f32 = 1.0;
pub const DEFAULT_FLAKE_SHININESS: f32 = 50.0;
pub const DEFAULT_FLAKE_CLEARCOAT_ROLLOFF: f32 = 2.2;
pub const DEFAULT_FLAKE_UVSCALE: f32 = 32.0;
pub const DEFAULT_FLAKE_VRATIO: f32 = 1.0;

/// Metallic flakes of a paint job.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Flake {
    /// `flake_color`.
    #[serde(default)]
    pub color: Option<[f32; 3]>,
    /// `flake_density` (0 to 2 in 1.60).
    #[serde(default)]
    pub density: Option<f32>,
    /// `flake_shininess` (0 to 60).
    #[serde(default)]
    pub shininess: Option<f32>,
    /// `flake_clearcoat_rolloff` (2.24 to 5.5).
    #[serde(default)]
    pub clearcoat_rolloff: Option<f32>,
    /// `flake_uvscale` (1 to 128); per-part units override it.
    #[serde(default)]
    pub uvscale: Option<f32>,
    /// `flake_noise` texture as written; see [`Flake::noise_texture`].
    #[serde(default)]
    pub noise: Option<String>,
}

impl Flake {
    /// The flake noise texture: `flake_noise`, else [`DEFAULT_FLAKE_NOISE`]
    /// (Hypothesis).
    pub fn noise_texture(&self) -> &str {
        self.noise.as_deref().unwrap_or(DEFAULT_FLAKE_NOISE)
    }
}

/// The colour shift seen at grazing angles.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Flip {
    /// `flip_color`.
    #[serde(default)]
    pub color: Option<[f32; 3]>,
    /// `flip_strength` (0.3 to 2.7).
    #[serde(default)]
    pub strength: Option<f32>,
}

/// The `*_locked` flags: a locked colour is the paint job's own, whatever
/// the truck's saved colour says (Confirmed, F17). Absent means false.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaintLocks {
    /// `base_color_locked`.
    #[serde(default)]
    pub base: bool,
    /// `mask_r_locked`, `mask_g_locked`, `mask_b_locked`.
    #[serde(default)]
    pub mask: [bool; 3],
    /// `flake_color_locked`.
    #[serde(default)]
    pub flake: bool,
    /// `flip_color_locked`.
    #[serde(default)]
    pub flip: bool,
}

/// One nameless `simple_paint_job_data` unit of a paint job's per-part file
/// (`<truck>/paint_job/accessory/<paint job token>.sii`): the accessories it
/// lists paint with its mask instead of the paint job's main mask.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PartPaint {
    /// `acc_list[]` entries (`<slot>.<accessory>`) that name an accessory of
    /// this truck (`<accessory>.<truck>.<slot>`).
    #[serde(default)]
    pub accessories: Vec<(SlotId, AccessoryId)>,
    /// `acc_list[]` entries that name no accessory of this truck.
    #[serde(default)]
    pub unresolved: Vec<String>,
    /// `paint_job_mask` (`.tobj`); see [`shared_mask_color`].
    #[serde(default)]
    pub mask: Option<String>,
    /// `flake_uvscale`.
    #[serde(default)]
    pub flake_uvscale: Option<f32>,
    /// `flake_vratio`; when written it equals the mask tile's height over
    /// width.
    #[serde(default)]
    pub flake_vratio: Option<f32>,
}

impl PartPaint {
    /// Whether `accessory` is in this part's `acc_list[]`.
    pub fn lists(&self, accessory: &AccessoryId) -> bool {
        self.accessories.iter().any(|(_, a)| a == accessory)
    }
}

/// A paint job (`accessory_paint_job_data`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PaintJob {
    /// `base_color`; absent on 448 of 19,998 units.
    #[serde(default)]
    pub base_color: Option<[f32; 3]>,
    /// `paint_job_mask`: the mask (or livery) texture of the main body.
    #[serde(default)]
    pub mask: Option<String>,
    /// `mask_r_color`, `mask_g_color`, `mask_b_color`. Units may write
    /// only some of the three.
    #[serde(default)]
    pub mask_colors: [Option<[f32; 3]>; 3],
    /// `airbrush`: the mask is a painted livery.
    #[serde(default)]
    pub airbrush: bool,
    /// `alternate_uvset`: the mask is mapped with a second UV set.
    #[serde(default)]
    pub alternate_uvset: bool,
    /// `flipflake`: flakes and flip colour are drawn.
    #[serde(default)]
    pub flipflake: bool,
    #[serde(default)]
    pub flake: Flake,
    #[serde(default)]
    pub flip: Flip,
    #[serde(default)]
    pub locks: PaintLocks,
    /// `stock` (meaning Unknown; 400 units write true, 320 false).
    #[serde(default)]
    pub stock: bool,
    /// `color_variant[]` in groups of four (the array length is always a
    /// multiple of four). What each of the four colours is, is Unknown.
    #[serde(default)]
    pub color_variants: Vec<[[f32; 3]; 4]>,
    /// Per-part overrides from the paint job's per-part file.
    #[serde(default)]
    pub parts: Vec<PartPaint>,
}

impl PaintJob {
    /// The per-part unit that lists `accessory`, if any.
    pub fn part_for(&self, accessory: &AccessoryId) -> Option<&PartPaint> {
        self.parts.iter().find(|p| p.lists(accessory))
    }
}

/// Decodes a constant-colour mask name: `.../shared_RRGGBBAA.tobj` is a
/// uniform 4x4 texture whose texels equal the hex digits (F15). Returns the
/// texel values in `[0, 1]`: RGB **sRGB-encoded** (the textures are BC1/BC3
/// sRGB, so sampling them yields `srgb_to_linear(rgb)`), alpha linear.
/// `None` for any other name.
pub fn shared_mask_color(path: &str) -> Option<[f32; 4]> {
    let file = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    let hex = file.strip_suffix(".tobj")?.strip_prefix("shared_")?;
    if hex.len() != 8 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut rgba = [0.0; 4];
    for (i, channel) in rgba.iter_mut().enumerate() {
        let byte = u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok()?;
        *channel = f32::from(byte) / 255.0;
    }
    Some(rgba)
}

/// The sRGB transfer function's inverse, for one channel in `[0, 1]`.
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
