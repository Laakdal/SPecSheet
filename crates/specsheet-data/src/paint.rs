//! The effective paint of one placed accessory: the selected paint job's
//! data merged with the configuration's colour overrides and the part's
//! per-part unit (finding F15, `docs/formats/paint.md`). Plain data for a
//! renderer; nothing here touches textures.

use specsheet_core::{AccessoryId, SlotId};
use specsheet_truck::{
    shared_mask_color, Configuration, TruckCatalog, DEFAULT_BASE_COLOR,
    DEFAULT_FLAKE_CLEARCOAT_ROLLOFF, DEFAULT_FLAKE_COLOR, DEFAULT_FLAKE_DENSITY,
    DEFAULT_FLAKE_SHININESS, DEFAULT_FLAKE_UVSCALE, DEFAULT_FLAKE_VRATIO, DEFAULT_FLIP_COLOR,
    DEFAULT_FLIP_STRENGTH, DEFAULT_MASK_COLORS,
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
    pub color: [f32; 3],
    pub density: f32,
    pub shininess: f32,
    pub clearcoat_rolloff: f32,
    /// The part's `flake_uvscale`, else the paint job's.
    pub uvscale: f32,
    /// The part's `flake_vratio` (only per-part units write it).
    pub vratio: f32,
    /// `flake_noise`, else `/material/custom/flake_noise.tobj` (F17).
    pub noise: String,
}

/// Flip parameters of a piece.
#[derive(Clone, Debug, PartialEq)]
pub struct FlipPaint {
    pub color: [f32; 3],
    pub strength: f32,
}

/// The effective paint of one placed accessory. Colours are the
/// configuration's overrides where set, else the paint job's (F15); a colour
/// the paint job locks is always the paint job's (F17). A value nobody sets
/// takes the game's default (`DEFAULT_*`, F17). Which
/// pieces of the accessory's model take it is decided by their material
/// (`eut2.truckpaint*` effects).
#[derive(Clone, Debug, PartialEq)]
pub struct PiecePaint {
    /// The selected paint job.
    pub paint_job: AccessoryId,
    /// Index into the paint job's `parts` when one lists this accessory.
    pub part: Option<usize>,
    pub base_color: [f32; 3],
    /// The part's mask when a part lists the accessory, else the paint
    /// job's main mask (Hypothesis for unlisted accessories).
    pub mask: Option<Mask>,
    /// Colours for the mask's red, green and blue channels.
    pub mask_colors: [[f32; 3]; 3],
    pub flake: FlakePaint,
    pub flip: FlipPaint,
    /// `airbrush`: the mask is a painted livery.
    pub airbrush: bool,
    /// `alternate_uvset`: the paint samples the alternate UV set
    /// ([`crate::render::paint_uv_set`]). Never for a listed part (F17).
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
    // The truck's saved colour wins unless the paint job locks that colour
    // (F17).
    let locks = paint.locks;
    let pick = |locked: bool, over: Option<[f32; 3]>, own: Option<[f32; 3]>| {
        if locked {
            own.or(over)
        } else {
            over.or(own)
        }
    };
    let mut mask_colors = DEFAULT_MASK_COLORS;
    for (i, color) in mask_colors.iter_mut().enumerate() {
        *color = pick(locks.mask[i], colors.mask[i], paint.mask_colors[i]).unwrap_or(*color);
    }
    let flake = &paint.flake;
    Some(PiecePaint {
        paint_job: job.id.clone(),
        part,
        base_color: pick(locks.base, colors.base, paint.base_color.or(job.base_color))
            .unwrap_or(DEFAULT_BASE_COLOR),
        mask,
        mask_colors,
        flake: FlakePaint {
            color: pick(locks.flake, colors.flake, flake.color).unwrap_or(DEFAULT_FLAKE_COLOR),
            density: flake.density.unwrap_or(DEFAULT_FLAKE_DENSITY),
            shininess: flake.shininess.unwrap_or(DEFAULT_FLAKE_SHININESS),
            clearcoat_rolloff: flake
                .clearcoat_rolloff
                .unwrap_or(DEFAULT_FLAKE_CLEARCOAT_ROLLOFF),
            uvscale: part_paint
                .and_then(|p| p.flake_uvscale)
                .or(flake.uvscale)
                .unwrap_or(DEFAULT_FLAKE_UVSCALE),
            vratio: part_paint
                .and_then(|p| p.flake_vratio)
                .unwrap_or(DEFAULT_FLAKE_VRATIO),
            noise: flake.noise_texture().to_owned(),
        },
        flip: FlipPaint {
            color: pick(locks.flip, colors.flip, paint.flip.color).unwrap_or(DEFAULT_FLIP_COLOR),
            strength: paint.flip.strength.unwrap_or(DEFAULT_FLIP_STRENGTH),
        },
        airbrush: paint.airbrush,
        alternate_uvset: paint.alternate_uvset && part.is_none(),
        flipflake: paint.flipflake,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use specsheet_core::TruckId;
    use specsheet_truck::{
        Accessory, AccessoryKind, Flake, Flip, ModelRef, PaintJob, PaintLocks, PaintOverride,
        PartPaint, DEFAULT_FLAKE_NOISE,
    };

    fn accessory(id: &str, slot: &str, kind: AccessoryKind) -> Accessory {
        Accessory {
            id: AccessoryId::from(id),
            slot: SlotId::from(slot),
            kind,
            def_path: format!("/def/t/{slot}/{id}.sii"),
            name: id.to_owned(),
            icon: None,
            price: None,
            unlock_level: None,
            model: ModelRef::default(),
            suitable_for: Vec::new(),
            conflict_with: Vec::new(),
            defaults: Vec::new(),
            requires: Vec::new(),
            base_color: None,
            wheel: None,
            left_hand_traffic: false,
            drive_twin: None,
            paint: None,
        }
    }

    const MAIN_MASK: &str = "/vehicle/truck/upgrade/paintjob/t/x/pjm_at_0x0_size_2048x2048.tobj";
    const PART_MASK: &str = "/vehicle/truck/upgrade/paintjob/t/x/shared_ff000080.tobj";

    fn setup() -> (TruckCatalog, Configuration) {
        setup_locked(PaintLocks::default())
    }

    fn setup_locked(locks: PaintLocks) -> (TruckCatalog, Configuration) {
        let mut job = accessory("x_a.t.paint_job", "paint_job", AccessoryKind::PaintJob);
        job.base_color = Some([0.5, 0.5, 0.5]);
        job.paint = Some(Box::new(PaintJob {
            base_color: Some([0.5, 0.5, 0.5]),
            mask: Some(MAIN_MASK.to_owned()),
            mask_colors: [Some([1.0, 0.0, 0.0]), Some([0.0, 1.0, 0.0]), None],
            airbrush: true,
            alternate_uvset: true,
            flake: Flake {
                color: Some([1.0, 1.0, 1.0]),
                uvscale: Some(24.0),
                ..Flake::default()
            },
            flip: Flip {
                color: Some([0.0, 0.0, 1.0]),
                strength: Some(1.7),
            },
            parts: vec![PartPaint {
                accessories: vec![("sunshld".into(), "stock.t.sunshld".into())],
                mask: Some(PART_MASK.to_owned()),
                flake_uvscale: Some(16.0),
                flake_vratio: Some(0.5),
                ..PartPaint::default()
            }],
            locks,
            ..PaintJob::default()
        }));
        let catalog = TruckCatalog::new(
            TruckId::from("t"),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            [
                job,
                accessory("high.t.cabin", "cabin", AccessoryKind::Cabin),
                accessory("stock.t.sunshld", "sunshld", AccessoryKind::Addon),
            ],
        );
        let config = Configuration::from_selections(
            TruckId::from("t"),
            [
                ("cabin".into(), "high.t.cabin".into()),
                ("sunshld".into(), "stock.t.sunshld".into()),
                ("paint_job".into(), "x_a.t.paint_job".into()),
            ],
        );
        (catalog, config)
    }

    #[test]
    fn unlisted_pieces_take_the_paint_jobs_own_values() {
        let (catalog, config) = setup();
        let paint =
            piece_paint(&catalog, &config, &"cabin".into(), &"high.t.cabin".into()).expect("paint");
        assert_eq!(paint.paint_job, AccessoryId::from("x_a.t.paint_job"));
        assert_eq!(paint.part, None);
        assert_eq!(paint.base_color, [0.5, 0.5, 0.5]);
        assert_eq!(paint.mask, Some(Mask::Texture(MAIN_MASK.to_owned())));
        assert_eq!(
            paint.mask_colors,
            [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            "mask b is the game's default blue"
        );
        assert_eq!(paint.flake.uvscale, 24.0);
        assert_eq!(paint.flake.vratio, 1.0);
        assert_eq!(paint.flake.noise, DEFAULT_FLAKE_NOISE);
        assert_eq!(paint.flip.strength, 1.7);
        assert!(paint.airbrush && paint.alternate_uvset && !paint.flipflake);
    }

    #[test]
    fn a_listed_piece_takes_its_parts_mask_and_flake_scale() {
        let (catalog, config) = setup();
        let paint = piece_paint(
            &catalog,
            &config,
            &"sunshld".into(),
            &"stock.t.sunshld".into(),
        )
        .expect("paint");
        assert_eq!(paint.part, Some(0));
        assert_eq!(
            paint.mask,
            Some(Mask::Constant {
                srgba: [1.0, 0.0, 0.0, 128.0 / 255.0],
                path: PART_MASK.to_owned(),
            })
        );
        assert_eq!(paint.mask.as_ref().map(Mask::path), Some(PART_MASK));
        assert_eq!(paint.flake.uvscale, 16.0);
        assert_eq!(paint.flake.vratio, 0.5);
        assert!(
            !paint.alternate_uvset,
            "a listed part never takes the alternate UV set (F17)"
        );
    }

    #[test]
    fn overrides_win_over_the_paint_jobs_colours() {
        let (catalog, mut config) = setup();
        config.set_paint_override(PaintOverride {
            base: Some([0.1, 0.1, 0.1]),
            mask: [None, Some([0.2, 0.2, 0.2]), Some([0.3, 0.3, 0.3])],
            flake: Some([0.4, 0.4, 0.4]),
            flip: None,
        });
        let paint =
            piece_paint(&catalog, &config, &"cabin".into(), &"high.t.cabin".into()).expect("paint");
        assert_eq!(paint.base_color, [0.1, 0.1, 0.1]);
        assert_eq!(
            paint.mask_colors,
            [[1.0, 0.0, 0.0], [0.2, 0.2, 0.2], [0.3, 0.3, 0.3]]
        );
        assert_eq!(paint.flake.color, [0.4, 0.4, 0.4]);
        assert_eq!(paint.flip.color, [0.0, 0.0, 1.0], "not overridden");
    }

    #[test]
    fn locked_colours_win_over_overrides() {
        let (catalog, mut config) = setup_locked(PaintLocks {
            base: true,
            mask: [true, false, false],
            flake: false,
            flip: true,
        });
        config.set_paint_override(PaintOverride {
            base: Some([0.1, 0.1, 0.1]),
            mask: [Some([0.2, 0.2, 0.2]), Some([0.3, 0.3, 0.3]), None],
            flake: Some([0.4, 0.4, 0.4]),
            flip: Some([0.5, 0.5, 0.5]),
        });
        let paint =
            piece_paint(&catalog, &config, &"cabin".into(), &"high.t.cabin".into()).expect("paint");
        assert_eq!(paint.base_color, [0.5, 0.5, 0.5], "locked");
        assert_eq!(
            paint.mask_colors,
            [[1.0, 0.0, 0.0], [0.3, 0.3, 0.3], [0.0, 0.0, 1.0]],
            "only mask r is locked"
        );
        assert_eq!(paint.flake.color, [0.4, 0.4, 0.4], "not locked");
        assert_eq!(paint.flip.color, [0.0, 0.0, 1.0], "locked");
    }

    #[test]
    fn values_a_paint_job_leaves_out_take_the_games_defaults() {
        let mut job = accessory("bare.t.paint_job", "paint_job", AccessoryKind::PaintJob);
        job.paint = Some(Box::new(PaintJob {
            flipflake: true,
            ..PaintJob::default()
        }));
        let catalog = TruckCatalog::new(
            TruckId::from("t"),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            [
                job,
                accessory("high.t.cabin", "cabin", AccessoryKind::Cabin),
            ],
        );
        let config = Configuration::from_selections(
            TruckId::from("t"),
            [
                ("cabin".into(), "high.t.cabin".into()),
                ("paint_job".into(), "bare.t.paint_job".into()),
            ],
        );
        let paint =
            piece_paint(&catalog, &config, &"cabin".into(), &"high.t.cabin".into()).expect("paint");
        assert_eq!(paint.base_color, [1.0, 1.0, 1.0]);
        assert_eq!(
            paint.mask_colors,
            [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
        );
        assert_eq!(
            paint.flip,
            FlipPaint {
                color: [1.0, 0.0, 0.0],
                strength: 0.27,
            }
        );
        assert_eq!(
            paint.flake,
            FlakePaint {
                color: [0.0, 1.0, 0.0],
                density: 1.0,
                shininess: 50.0,
                clearcoat_rolloff: 2.2,
                uvscale: 32.0,
                vratio: 1.0,
                noise: DEFAULT_FLAKE_NOISE.to_owned(),
            }
        );
    }

    #[test]
    fn no_paint_job_means_no_paint() {
        let (catalog, _) = setup();
        let config = Configuration::from_selections(
            TruckId::from("t"),
            [("cabin".into(), "high.t.cabin".into())],
        );
        assert_eq!(
            piece_paint(&catalog, &config, &"cabin".into(), &"high.t.cabin".into()),
            None
        );
    }
}
