//! Packed textures. A HashFS v2 entry can hold a texture as an image record
//! (surface layout), a sample record (sampler state) and the decompressed
//! surface bytes, padded per the recorded alignments.
//! Format: `docs/formats/textures.md`.

/// Image record: dimensions and layout of the packed surface data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageMeta {
    /// Width of mip 0 in pixels.
    pub width: u32,
    /// Height of mip 0 in pixels.
    pub height: u32,
    /// Number of mip levels.
    pub mip_count: u32,
    /// `DXGI_FORMAT` value of the surfaces.
    pub dxgi_format: u32,
    /// A cube map (six faces).
    pub is_cube: bool,
    /// Number of faces: 1, or 6 for a cube.
    pub face_count: u32,
    /// Row alignment in bytes.
    pub pitch_alignment: u32,
    /// Per-mip alignment in bytes.
    pub image_alignment: u32,
}

/// Extracts `width` bits of `word` starting at bit `shift`.
fn bits(word: u32, shift: u32, width: u32) -> u32 {
    (word >> shift) & ((1 << width) - 1)
}

impl ImageMeta {
    /// Decodes the image record's two little-endian `u32` words, in file order.
    ///
    /// The first word holds width − 1 and height − 1; the second the flags
    /// (textures.md, "Image flags").
    pub(crate) fn decode(words: [u32; 2]) -> Self {
        let [dims, flags] = words;
        Self {
            width: bits(dims, 0, 16) + 1,
            height: bits(dims, 16, 16) + 1,
            mip_count: bits(flags, 0, 4) + 1,
            dxgi_format: bits(flags, 4, 8),
            is_cube: bits(flags, 12, 2) != 0,
            face_count: bits(flags, 14, 6) + 1,
            pitch_alignment: 1 << bits(flags, 20, 4),
            image_alignment: 1 << bits(flags, 24, 4),
        }
    }
}

/// Texture addressing mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddressMode {
    /// Wrap around.
    Repeat,
    /// Clamp (legacy mode).
    Clamp,
    /// Clamp to the edge texel.
    ClampToEdge,
    /// Clamp to the border colour.
    ClampToBorder,
    /// Mirror on every repeat.
    Mirror,
    /// Mirror once, then clamp.
    MirrorClamp,
    /// Mirror once, then clamp to the edge texel.
    MirrorClampToEdge,
}

impl AddressMode {
    /// Maps a 3-bit address field (textures.md, "Sample record"). The value
    /// 7 has no mode and falls back to `Repeat`.
    fn from_bits(value: u32) -> Self {
        match value {
            0 => Self::Repeat,
            1 => Self::Clamp,
            2 => Self::ClampToEdge,
            3 => Self::ClampToBorder,
            4 => Self::Mirror,
            5 => Self::MirrorClamp,
            6 => Self::MirrorClampToEdge,
            // 7 is unassigned.
            _ => Self::Repeat,
        }
    }
}

/// Sample record: the texture's sampler state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SampleMeta {
    /// Linear magnification filter (else nearest).
    pub mag_linear: bool,
    /// Linear minification filter (else nearest).
    pub min_linear: bool,
    /// Addressing along u.
    pub address_u: AddressMode,
    /// Addressing along v.
    pub address_v: AddressMode,
}

impl SampleMeta {
    /// Decodes the sample record's `u32` word.
    pub(crate) fn decode(word: u32) -> Self {
        Self {
            mag_linear: bits(word, 0, 1) != 0,
            min_linear: bits(word, 1, 1) != 0,
            address_u: AddressMode::from_bits(bits(word, 4, 3)),
            address_v: AddressMode::from_bits(bits(word, 7, 3)),
        }
    }
}

/// A texture read from an archive: its metadata plus the decompressed
/// surface bytes, still padded. [`PackedTexture::unpack`] removes the padding.
#[derive(Clone, Debug, PartialEq)]
pub struct PackedTexture {
    /// Surface layout.
    pub image: ImageMeta,
    /// Sampler state.
    pub sample: SampleMeta,
    /// Every surface, decompressed, with the archive's padding.
    pub surfaces: Vec<u8>,
}

/// One mip level of one face, tightly packed (no row or mip padding).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Surface {
    /// Cube face index (0 for 2D textures).
    pub face: u32,
    /// Mip level (0 is the largest).
    pub mip: u32,
    /// Width of this mip level in pixels (never below 1).
    pub width: u32,
    /// Height of this mip level in pixels (never below 1).
    pub height: u32,
    /// The pixels, row after row without padding.
    pub data: Vec<u8>,
}

/// How a DXGI format stores its pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelLayout {
    /// Block-compressed: 4x4-pixel blocks of `bytes` bytes each.
    Block {
        /// Bytes per block.
        bytes: u32,
    },
    /// Uncompressed: `bytes_per_pixel` bytes per pixel.
    Linear {
        /// Bytes per pixel.
        bytes_per_pixel: u32,
    },
}

/// Layout of a DXGI format that appears in the game's archives (see
/// `docs/formats/textures.md` for the formats observed). `None` for formats
/// this crate cannot unpack.
///
/// Each observed format is covered together with its DXGI family (typeless,
/// UNORM, sRGB, integer variants), whose members share one memory layout.
pub fn pixel_layout(dxgi_format: u32) -> Option<PixelLayout> {
    let layout = match dxgi_format {
        // R8G8B8A8, R16G16 and B8G8R8A8/X8 families: 4 bytes per pixel.
        27..=38 | 87 | 88 | 90..=93 => PixelLayout::Linear { bytes_per_pixel: 4 },
        // R9G9B9E5_SHAREDEXP: three 9-bit mantissas and a shared exponent.
        67 => PixelLayout::Linear { bytes_per_pixel: 4 },
        // R8 family and A8_UNORM.
        60..=65 => PixelLayout::Linear { bytes_per_pixel: 1 },
        // BC1 and BC4: 8 bytes per 4x4 block.
        70..=72 | 79..=81 => PixelLayout::Block { bytes: 8 },
        // BC2, BC3 and BC5: 16 bytes per 4x4 block.
        73..=78 | 82..=84 => PixelLayout::Block { bytes: 16 },
        _ => return None,
    };
    Some(layout)
}

/// Rows and bytes per row of one mip level: block rows for BC formats.
fn mip_rows(layout: PixelLayout, width: u32, height: u32) -> Option<(u64, u64)> {
    let (rows, row_bytes) = match layout {
        PixelLayout::Block { bytes } => (
            u64::from(height.div_ceil(4)),
            u64::from(width.div_ceil(4)).checked_mul(u64::from(bytes))?,
        ),
        PixelLayout::Linear { bytes_per_pixel } => (
            u64::from(height),
            u64::from(width).checked_mul(u64::from(bytes_per_pixel))?,
        ),
    };
    Some((rows, row_bytes))
}

/// `offset` rounded up to a multiple of `alignment`; 0 means no alignment.
fn align_up(offset: u64, alignment: u64) -> Option<u64> {
    if alignment <= 1 {
        Some(offset)
    } else {
        offset.checked_next_multiple_of(alignment)
    }
}

/// Size of mip `mip` along one axis: halved per level, never below 1.
fn mip_extent(base: u32, mip: u32) -> u32 {
    base.checked_shr(mip).unwrap_or(0).max(1)
}

impl PackedTexture {
    /// Removes the alignment padding. Returns the surfaces in face-major
    /// order (all mips of face 0, then face 1, ...), mips largest first.
    /// Mip `n` is the base size halved `n` times, never below 1 pixel. An
    /// alignment of 0 means no padding. An unsupported format, or surface
    /// data too short for the metadata, returns an error message; trailing
    /// extra bytes are allowed. Never panics.
    ///
    /// Padding model (textures.md, "Surface data"): every mip starts on the
    /// image alignment and every row (block row) on the pitch alignment;
    /// padding only ever precedes a mip or a row.
    pub fn unpack(&self) -> Result<Vec<Surface>, String> {
        let image = &self.image;
        let layout = pixel_layout(image.dxgi_format)
            .ok_or_else(|| format!("unsupported DXGI format {}", image.dxgi_format))?;
        let data = self.surfaces.as_slice();
        let available = data.len() as u64;
        let pitch = u64::from(image.pitch_alignment);
        let mip_alignment = u64::from(image.image_alignment);
        let overflow = || "surface layout overflows".to_owned();
        let too_short = |face: u32, mip: u32, end: u64| {
            format!(
                "surface data too short: face {face} mip {mip} ends at {end}, have {available} bytes"
            )
        };

        let mut surfaces = Vec::new();
        let mut offset = 0u64;
        for face in 0..image.face_count {
            for mip in 0..image.mip_count {
                let width = mip_extent(image.width, mip);
                let height = mip_extent(image.height, mip);
                let (rows, row_bytes) = mip_rows(layout, width, height).ok_or_else(overflow)?;

                // Every row after the first starts pitch-aligned, so rows
                // sit a fixed `row_stride` apart.
                let first_row = align_up(offset, mip_alignment)
                    .and_then(|start| align_up(start, pitch))
                    .ok_or_else(overflow)?;
                let row_stride = align_up(row_bytes, pitch).ok_or_else(overflow)?;
                let end = rows
                    .checked_sub(1)
                    .and_then(|last| last.checked_mul(row_stride))
                    .and_then(|span| span.checked_add(first_row))
                    .and_then(|last_row| last_row.checked_add(row_bytes))
                    .ok_or_else(overflow)?;
                if end > available {
                    return Err(too_short(face, mip, end));
                }

                // `end` fits in the slice, so every row offset below fits in
                // `usize` and in bounds.
                let mut pixels = Vec::with_capacity(usize::try_from(rows * row_bytes).unwrap_or(0));
                for row in 0..rows {
                    let start = first_row + row * row_stride;
                    let slice = usize::try_from(start)
                        .ok()
                        .zip(usize::try_from(start + row_bytes).ok())
                        .and_then(|(from, to)| data.get(from..to))
                        .ok_or_else(|| too_short(face, mip, end))?;
                    pixels.extend_from_slice(slice);
                }
                surfaces.push(Surface {
                    face,
                    mip,
                    width,
                    height,
                    data: pixels,
                });
                offset = end;
            }
        }
        Ok(surfaces)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BC3_SRGB: u32 = 78;

    fn meta(width: u32, height: u32, mips: u32, dxgi: u32, faces: u32) -> ImageMeta {
        ImageMeta {
            width,
            height,
            mip_count: mips,
            dxgi_format: dxgi,
            is_cube: faces == 6,
            face_count: faces,
            pitch_alignment: 256,
            image_alignment: 512,
        }
    }

    fn packed(image: ImageMeta, len: usize) -> PackedTexture {
        PackedTexture {
            image,
            sample: SampleMeta::decode(0x3),
            surfaces: (0..len).map(|i| i as u8).collect(),
        }
    }

    #[test]
    fn decodes_the_documented_bc5_example() {
        // textures.md, "Image flags": 0x09800537 is 128², 8 mips, BC5,
        // 256-byte pitch and 512-byte image alignment.
        let image = ImageMeta::decode([0x007f_007f, 0x0980_0537]);
        assert_eq!(
            image,
            ImageMeta {
                width: 128,
                height: 128,
                mip_count: 8,
                dxgi_format: 83,
                is_cube: false,
                face_count: 1,
                pitch_alignment: 256,
                image_alignment: 512,
            }
        );
    }

    #[test]
    fn decodes_a_non_square_record_and_a_cube_flag() {
        let image = ImageMeta::decode([0x0003_00ff, 0x0980_04e8]);
        assert_eq!((image.width, image.height), (256, 4));
        assert_eq!((image.mip_count, image.dxgi_format), (9, 78));
        assert!(!image.is_cube);
        assert_eq!(image.face_count, 1);

        // Cube flag in bits 12–13, face count − 1 (5) in bits 14–19.
        let cube = ImageMeta::decode([0, (5 << 14) | (1 << 12) | (72 << 4)]);
        assert!(cube.is_cube);
        assert_eq!(cube.face_count, 6);
        assert_eq!((cube.pitch_alignment, cube.image_alignment), (1, 1));
    }

    #[test]
    fn decodes_sampler_state() {
        let linear_repeat = SampleMeta::decode(0b11);
        assert!(linear_repeat.mag_linear && linear_repeat.min_linear);
        assert_eq!(linear_repeat.address_u, AddressMode::Repeat);
        assert_eq!(linear_repeat.address_v, AddressMode::Repeat);

        // Skybox panorama: repeat U, clamp-to-edge V.
        let skybox = SampleMeta::decode(0b11 | (2 << 7));
        assert_eq!(skybox.address_u, AddressMode::Repeat);
        assert_eq!(skybox.address_v, AddressMode::ClampToEdge);

        let point_border = SampleMeta::decode((3 << 4) | (3 << 7));
        assert!(!point_border.mag_linear && !point_border.min_linear);
        assert_eq!(point_border.address_u, AddressMode::ClampToBorder);
        assert_eq!(point_border.address_v, AddressMode::ClampToBorder);

        // Mip filter (bits 2–3) and address W (bits 10–12) are ignored.
        assert_eq!(SampleMeta::decode(0x1c0c | 0b11), linear_repeat);
        assert_eq!(
            SampleMeta::decode(6 << 4).address_u,
            AddressMode::MirrorClampToEdge
        );
        assert_eq!(SampleMeta::decode(7 << 4).address_u, AddressMode::Repeat);
    }

    #[test]
    fn knows_the_observed_formats() {
        for dxgi in [35, 67, 87, 88, 91, 93] {
            assert_eq!(
                pixel_layout(dxgi),
                Some(PixelLayout::Linear { bytes_per_pixel: 4 })
            );
        }
        assert_eq!(
            pixel_layout(61),
            Some(PixelLayout::Linear { bytes_per_pixel: 1 })
        );
        for dxgi in [71, 72, 80] {
            assert_eq!(pixel_layout(dxgi), Some(PixelLayout::Block { bytes: 8 }));
        }
        for dxgi in [75, 77, 78, 83] {
            assert_eq!(pixel_layout(dxgi), Some(PixelLayout::Block { bytes: 16 }));
        }
        for dxgi in [0, 2, 66, 89, 98, u32::MAX] {
            assert_eq!(pixel_layout(dxgi), None);
        }
    }

    #[test]
    fn predicts_the_sizes_measured_in_the_archives() {
        // research/evidence/texture-unpack-derivation.txt, "Examples".
        for (width, height, mips, size) in [
            (512, 512, 10, 352_784),
            (128, 64, 8, 13_328),
            (256, 64, 9, 24_080),
        ] {
            let image = meta(width, height, mips, BC3_SRGB, 1);
            let surfaces = packed(image, size).unpack();
            let surfaces = surfaces.unwrap_or_else(|e| panic!("{width}x{height}: {e}"));
            assert_eq!(surfaces.len() as u32, mips);
            assert!(packed(image, size - 1).unpack().is_err());
        }
    }

    #[test]
    fn strips_row_and_mip_padding() {
        // 8x8 BC1, 3 mips, pitch 16, image alignment 64. Block rows are
        // 16, 8 and 8 bytes long.
        let image = ImageMeta {
            pitch_alignment: 16,
            image_alignment: 64,
            ..meta(8, 8, 3, 71, 1)
        };
        // mip 0: rows at 0 and 16 (16 bytes each), ends at 32.
        // mip 1: starts at 64, one 8-byte row, ends at 72.
        // mip 2: starts at 128, one 8-byte row, ends at 136.
        let surfaces = packed(image, 136).unpack().unwrap();
        let bytes = |from: u8, len: u8| (from..from + len).collect::<Vec<u8>>();
        assert_eq!(surfaces[0].data, bytes(0, 32));
        assert_eq!((surfaces[1].width, surfaces[1].height), (4, 4));
        assert_eq!(surfaces[1].data, bytes(64, 8));
        // The 2x2 tail mip still takes a whole 4x4 block.
        assert_eq!((surfaces[2].width, surfaces[2].height), (2, 2));
        assert_eq!(surfaces[2].data, bytes(128, 8));
    }

    #[test]
    fn linear_rows_are_padded_to_the_pitch() {
        // 3x2 R8, pitch 4: row 0 at 0..3, row 1 at 4..7.
        let image = ImageMeta {
            pitch_alignment: 4,
            image_alignment: 0,
            ..meta(3, 2, 1, 61, 1)
        };
        let surfaces = packed(image, 7).unpack().unwrap();
        assert_eq!(surfaces[0].data, vec![0, 1, 2, 4, 5, 6]);
    }

    #[test]
    fn zero_alignment_means_tightly_packed() {
        let image = ImageMeta {
            pitch_alignment: 0,
            image_alignment: 0,
            ..meta(2, 2, 2, 87, 1)
        };
        // 16 bytes for 2x2, then 4 bytes for 1x1.
        let surfaces = packed(image, 20).unpack().unwrap();
        assert_eq!(surfaces[0].data.len(), 16);
        assert_eq!(surfaces[1].data, vec![16, 17, 18, 19]);
        assert!(packed(image, 19).unpack().is_err());
    }

    #[test]
    fn cube_faces_are_face_major() {
        // 4x4 BC1 cube, 2 mips: each face is a 4x4 and a 2x2 mip, one
        // 8-byte block each, every mip on a 32-byte boundary.
        let image = ImageMeta {
            pitch_alignment: 0,
            image_alignment: 32,
            ..meta(4, 4, 2, 72, 6)
        };
        let size = 11 * 32 + 8;
        let surfaces = packed(image, size).unpack().unwrap();
        let order: Vec<(u32, u32)> = surfaces.iter().map(|s| (s.face, s.mip)).collect();
        let expected: Vec<(u32, u32)> = (0..6).flat_map(|face| [(face, 0), (face, 1)]).collect();
        assert_eq!(order, expected);
        assert!(surfaces.iter().all(|s| s.data.len() == 8));
        // Face 1 starts with the third mip, at 2 * 32.
        assert_eq!(surfaces[2].data[0], 64);
        assert_eq!(surfaces[11].data[0], (11 * 32 % 256) as u8);
        assert!(packed(image, size - 1).unpack().is_err());
    }

    #[test]
    fn mips_never_drop_below_one_pixel() {
        // 8x2 with 4 mips: 8x2, 4x1, 2x1, 1x1.
        let image = ImageMeta {
            pitch_alignment: 0,
            image_alignment: 0,
            ..meta(8, 2, 4, 61, 1)
        };
        let surfaces = packed(image, 16 + 4 + 2 + 1).unpack().unwrap();
        let sizes: Vec<(u32, u32)> = surfaces.iter().map(|s| (s.width, s.height)).collect();
        assert_eq!(sizes, [(8, 2), (4, 1), (2, 1), (1, 1)]);
    }

    #[test]
    fn allows_trailing_bytes() {
        let image = meta(4, 4, 1, 71, 1);
        let surfaces = packed(image, 1000).unpack().unwrap();
        assert_eq!(surfaces[0].data, (0..8).collect::<Vec<u8>>());
    }

    #[test]
    fn rejects_unsupported_formats_and_short_data() {
        assert!(packed(meta(4, 4, 1, 66, 1), 4096).unpack().is_err());
        assert!(packed(meta(4, 4, 1, 71, 1), 7).unpack().is_err());
        assert!(packed(meta(4, 4, 1, 71, 1), 0).unpack().is_err());
    }

    #[test]
    fn hostile_metadata_fails_without_panicking() {
        let extremes = [
            meta(u32::MAX, u32::MAX, u32::MAX, 87, u32::MAX),
            meta(u32::MAX, 1, 1, 77, 1),
            ImageMeta {
                pitch_alignment: u32::MAX,
                image_alignment: u32::MAX,
                ..meta(64, 64, 40, 71, 1)
            },
            ImageMeta {
                pitch_alignment: 1 << 31,
                image_alignment: 3,
                ..meta(4, 4, 2, 71, 2)
            },
        ];
        for image in extremes {
            assert!(packed(image, 4096).unpack().is_err(), "{image:?}");
        }
        // 40 mips of a 1x1 texture: shifts past 31 bits still give 1 pixel.
        let tiny = ImageMeta {
            pitch_alignment: 0,
            image_alignment: 0,
            ..meta(1, 1, 40, 61, 1)
        };
        assert_eq!(packed(tiny, 40).unpack().map(|s| s.len()), Ok(40));
        let empty = ImageMeta {
            face_count: 0,
            ..meta(4, 4, 1, 71, 0)
        };
        assert_eq!(packed(empty, 0).unpack(), Ok(Vec::new()));
    }
}
