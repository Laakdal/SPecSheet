//! The effective paint of one placed accessory: the selected paint job's
//! data merged with the configuration's colour overrides and the part's
//! per-part unit (finding F15, `docs/formats/paint.md`). Plain data for a
//! renderer; nothing here touches textures.

use specsheet_core::{
    shared_mask_color, AccessoryId, Configuration, SlotId, TruckCatalog, DEFAULT_FLAKE_NOISE,
};

/// Where a piece's mask comes from.
#[derive(Clone, Debug, PartialEq)]
pub enum Mask {
    /// A `.tobj` mask (or livery) texture.
    Texture(String),
    /// A `shared_RRGGBBAA` mask: a uniform colour, so no texture is needed.
    /// RGB sRGB-encoded, alpha linear, as [`shared_mask_color`] returns.
    /// `path` is the texture the game would load.
    Constant { srgba: [f32; 4], path: String },
}

impl Mask {
    fn from_path(path: &str) -> Self {
        match shared_mask_color(path) {
            Some(srgba) => Self::Constant {
                srgba,
                path: path.to_owned(),
            },
            None => Self::Texture(path.to_owned()),
        }
    }

    /// The texture path either way.
    pub fn path(&self) -> &str {
        match self {
            Self::Texture(path) | Self::Constant { path, .. } => path,
        }
    }
}

/// Flake parameters of a piece.
#[derive(Clone, Debug, PartialEq)]
pub struct FlakePaint {
    pub color: Option<[f32; 3]>,
    pub density: Option<f32>,
    pub shininess: Option<f32>,
    pub clearcoat_rolloff: Option<f32>,
    /// The part's `flake_uvscale`, else the paint job's.
    pub uvscale: Option<f32>,
    /// The part's `flake_vratio` (only per-part units write it).
    pub vratio: Option<f32>,
    /// `flake_noise`, else `/material/custom/flake_noise.tobj` (Hypothesis).
    pub noise: String,
}

/// Flip parameters of a piece.
#[derive(Clone, Debug, PartialEq)]
pub struct FlipPaint {
    pub color: Option<[f32; 3]>,
    pub strength: Option<f32>,
}

/// The effective paint of one placed accessory. Colours are the
/// configuration's overrides where set, else the paint job's (F15). Which
/// pieces of the accessory's model take it is decided by their material
/// (`eut2.truckpaint*` effects).
#[derive(Clone, Debug, PartialEq)]
pub struct PiecePaint {
    /// The selected paint job.
    pub paint_job: AccessoryId,
    /// Index into the paint job's `parts` when one lists this accessory.
    pub part: Option<usize>,
    pub base_color: Option<[f32; 3]>,
    /// The part's mask when a part lists the accessory, else the paint
    /// job's main mask (Hypothesis for unlisted accessories).
    pub mask: Option<Mask>,
    /// Colours for the mask's red, green and blue channels.
    pub mask_colors: [Option<[f32; 3]>; 3],
    pub flake: FlakePaint,
    pub flip: FlipPaint,
    /// `airbrush`: the mask is a painted livery.
    pub airbrush: bool,
    /// `alternate_uvset`: the mask uses the second UV set.
    pub alternate_uvset: bool,
    /// `flipflake`: flakes and flip colour are drawn.
    pub flipflake: bool,
}

/// The paint of the accessory `accessory` placed in `slot`, or `None` when
/// no paint job is selected (or its data is missing).
pub fn piece_paint(
    catalog: &TruckCatalog,
    config: &Configuration,
    slot: &SlotId,
    accessory: &AccessoryId,
) -> Option<PiecePaint> {
    let job = config.paint_job(catalog)?;
    let paint = job.paint.as_deref()?;
    let colors = config.paint_override();
    let found = paint.parts.iter().enumerate().find(|(_, part)| {
        part.accessories
            .iter()
            .any(|(s, a)| s == slot && a == accessory)
    });
    let (part, part_paint) = match found {
        Some((index, part)) => (Some(index), Some(part)),
        None => (None, None),
    };
    let mask = part_paint
        .and_then(|p| p.mask.as_deref())
        .or(paint.mask.as_deref())
        .map(Mask::from_path);
    let mut mask_colors = paint.mask_colors;
    for (color, over) in mask_colors.iter_mut().zip(colors.mask) {
        *color = over.or(*color);
    }
    Some(PiecePaint {
        paint_job: job.id.clone(),
        part,
        base_color: colors.base.or(paint.base_color).or(job.base_color),
        mask,
        mask_colors,
        flake: FlakePaint {
            color: colors.flake.or(paint.flake.color),
            density: paint.flake.density,
            shininess: paint.flake.shininess,
            clearcoat_rolloff: paint.flake.clearcoat_rolloff,
            uvscale: part_paint
                .and_then(|p| p.flake_uvscale)
                .or(paint.flake.uvscale),
            vratio: part_paint.and_then(|p| p.flake_vratio),
            noise: paint
                .flake
                .noise
                .clone()
                .unwrap_or_else(|| DEFAULT_FLAKE_NOISE.to_owned()),
        },
        flip: FlipPaint {
            color: colors.flip.or(paint.flip.color),
            strength: paint.flip.strength,
        },
        airbrush: paint.airbrush,
        alternate_uvset: paint.alternate_uvset,
        flipflake: paint.flipflake,
    })
}
