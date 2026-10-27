//! Packed textures. HashFS v2 stores a `.tobj` and its `.dds` as one entry:
//! an image record (surface layout), a sample record (sampler state) and the
//! surface bytes with every mip level / row padded to the recorded alignment.

/// Image record: surface dimensions and layout of the packed pixel data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageMeta {
    pub width: u32,
    pub height: u32,
    pub mip_count: u32,
    /// `DXGI_FORMAT` value of the surfaces.
    pub dxgi_format: u32,
    pub is_cube: bool,
    pub face_count: u32,
    pub pitch_alignment: u32,
    pub image_alignment: u32,
}

impl ImageMeta {
    pub(crate) fn decode([dims, flags]: [u32; 2]) -> Self {
        let bits = |start: u32, len: u32| (flags >> start) & ((1 << len) - 1);
        Self {
            width: (dims & 0xffff) + 1,
            height: (dims >> 16) + 1,
            mip_count: bits(0, 4) + 1,
            dxgi_format: bits(4, 8),
            is_cube: bits(12, 2) != 0,
            face_count: bits(14, 6) + 1,
            pitch_alignment: 1 << bits(20, 4),
            image_alignment: 1 << bits(24, 4),
        }
    }
}

/// Texture addressing mode (the `.tobj` `addr_*` values).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddressMode {
    Repeat,
    Clamp,
    ClampToEdge,
    ClampToBorder,
    Mirror,
    MirrorClamp,
    MirrorClampToEdge,
    Other(u8),
}

impl AddressMode {
    fn from_bits(value: u32) -> Self {
        match value {
            0 => Self::Repeat,
            1 => Self::Clamp,
            2 => Self::ClampToEdge,
            3 => Self::ClampToBorder,
            4 => Self::Mirror,
            5 => Self::MirrorClamp,
            6 => Self::MirrorClampToEdge,
            other => Self::Other(other as u8),
        }
    }
}

/// Sample record: the sampler state of the original `.tobj`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SampleMeta {
    pub mag_linear: bool,
    pub min_linear: bool,
    pub address_u: AddressMode,
    pub address_v: AddressMode,
}

impl SampleMeta {
    pub(crate) fn decode(word: u32) -> Self {
        Self {
            mag_linear: word & 1 != 0,
            min_linear: word & 2 != 0,
            address_u: AddressMode::from_bits((word >> 4) & 0x7),
            address_v: AddressMode::from_bits((word >> 7) & 0x7),
        }
    }
}

/// A texture read out of an archive: metadata plus the decompressed but
/// still padded surface bytes. Use [`PackedTexture::unpack`] to obtain
/// tightly packed mip levels.
#[derive(Clone, Debug, PartialEq)]
pub struct PackedTexture {
    pub image: ImageMeta,
    pub sample: SampleMeta,
    pub surfaces: Vec<u8>,
}

/// One tightly packed mip level of one face.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Surface {
    pub face: u32,
    pub mip: u32,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

/// Bytes per 4x4 block (block-compressed) or per pixel (uncompressed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelLayout {
    Block { bytes: u32 },
    Linear { bytes_per_pixel: u32 },
}

/// Layout of the DXGI formats that appear in game archives.
pub fn pixel_layout(dxgi_format: u32) -> Option<PixelLayout> {
    use PixelLayout::{Block, Linear};
    Some(match dxgi_format {
        70..=72 | 79..=81 => Block { bytes: 8 }, // BC1, BC4
        73..=78 | 82..=84 | 94..=99 => Block { bytes: 16 }, // BC2, BC3, BC5, BC6H, BC7
        2 => Linear {
            bytes_per_pixel: 16,
        }, // R32G32B32A32_FLOAT
        10 => Linear { bytes_per_pixel: 8 },     // R16G16B16A16_FLOAT
        27..=32 | 87..=93 => Linear { bytes_per_pixel: 4 }, // RGBA8 / BGRA8 / BGRX8
        33..=38 => Linear { bytes_per_pixel: 4 }, // R16G16
        48..=52 => Linear { bytes_per_pixel: 2 }, // R8G8
        60..=64 => Linear { bytes_per_pixel: 1 }, // R8 / A8
        _ => return None,
    })
}

impl PackedTexture {
    /// Removes the per-mip and per-row alignment padding, yielding surfaces
    /// ordered face-major then mip-major (the DDS order).
    pub fn unpack(&self) -> Result<Vec<Surface>, String> {
        let image = &self.image;
        let layout = pixel_layout(image.dxgi_format)
            .ok_or_else(|| format!("unsupported DXGI format {}", image.dxgi_format))?;
        let mut surfaces = Vec::with_capacity((image.face_count * image.mip_count) as usize);
        let mut offset = 0usize;
        for face in 0..image.face_count {
            let (mut width, mut height) = (image.width, image.height);
            for mip in 0..image.mip_count {
                let (row_bytes, rows) = match layout {
                    PixelLayout::Block { bytes } => {
                        (width.div_ceil(4).max(1) * bytes, height.div_ceil(4).max(1))
                    }
                    PixelLayout::Linear { bytes_per_pixel } => (width * bytes_per_pixel, height),
                };
                offset = align(offset, image.image_alignment);
                let mut data = Vec::with_capacity((row_bytes * rows) as usize);
                for _ in 0..rows {
                    offset = align(offset, image.pitch_alignment);
                    let row = self
                        .surfaces
                        .get(offset..offset + row_bytes as usize)
                        .ok_or_else(|| {
                            format!("surface data truncated at face {face} mip {mip}")
                        })?;
                    data.extend_from_slice(row);
                    offset += row_bytes as usize;
                }
                surfaces.push(Surface {
                    face,
                    mip,
                    width,
                    height,
                    data,
                });
                width = (width / 2).max(1);
                height = (height / 2).max(1);
            }
        }
        Ok(surfaces)
    }
}

fn align(value: usize, alignment: u32) -> usize {
    value.next_multiple_of(alignment.max(1) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_image_record_observed_in_base_vehicle() {
        // 128x128 BC5_UNORM, 8 mips, 256-byte pitch, 512-byte image alignment.
        let meta = ImageMeta::decode([0x007f_007f, 0x0980_0537]);
        assert_eq!((meta.width, meta.height), (128, 128));
        assert_eq!(meta.mip_count, 8);
        assert_eq!(meta.dxgi_format, 83);
        assert_eq!(meta.face_count, 1);
        assert!(!meta.is_cube);
        assert_eq!(meta.pitch_alignment, 256);
        assert_eq!(meta.image_alignment, 512);
    }

    #[test]
    fn unpack_strips_row_and_mip_padding() {
        // 8x8 BC1 (8 bytes per block): mip0 is 2 block rows of 16 bytes, mip1 is
        // one row of 8 bytes. Pitch alignment 32, image alignment 64.
        let image = ImageMeta {
            width: 8,
            height: 8,
            mip_count: 2,
            dxgi_format: 71,
            is_cube: false,
            face_count: 1,
            pitch_alignment: 32,
            image_alignment: 64,
        };
        let mut packed = vec![0u8; 128];
        packed[0..16].fill(1); // mip0 row0
        packed[32..48].fill(2); // mip0 row1
        packed[64..72].fill(3); // mip1 row0 (8 bytes: one 4x4 block)
        let texture = PackedTexture {
            image,
            sample: SampleMeta::decode(0),
            surfaces: packed,
        };
        let surfaces = texture.unpack().expect("unpack");
        assert_eq!(surfaces.len(), 2);
        assert_eq!(surfaces[0].data, [vec![1; 16], vec![2; 16]].concat());
        assert_eq!(surfaces[1].data, vec![3; 8]);
        assert_eq!((surfaces[1].width, surfaces[1].height), (4, 4));
    }

    #[test]
    fn unpack_reports_truncated_data() {
        let texture = PackedTexture {
            image: ImageMeta::decode([0x007f_007f, 0x0980_0537]),
            sample: SampleMeta::decode(0),
            surfaces: vec![0; 100],
        };
        assert!(texture.unpack().is_err());
    }
}
