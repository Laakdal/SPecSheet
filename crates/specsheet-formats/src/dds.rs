//! DirectDraw Surface (`.dds`) files, as mod packages hold them next to a
//! standalone `.tobj` (D20). The layout is Microsoft's published DDS format:
//! `"DDS "`, a 124-byte header, an optional 20-byte DX10 header, then the
//! surfaces face by face, each face's mips largest first, tightly packed.
//! Formats seen in mods are in `research/evidence/tobj-loose.txt` §3.

use crate::FormatError;

const MAGIC: &[u8; 4] = b"DDS ";
const HEADER_LEN: usize = 124;
const DX10_LEN: usize = 20;

// Header fields (offsets from the start of the file).
const FLAGS_AT: usize = 8;
const HEIGHT_AT: usize = 12;
const WIDTH_AT: usize = 16;
const MIPS_AT: usize = 28;
const PF_FLAGS_AT: usize = 80;
const FOURCC_AT: usize = 84;
const BITS_AT: usize = 88;
const R_MASK_AT: usize = 92;
const CAPS2_AT: usize = 112;
// DX10 header fields.
const DXGI_AT: usize = 128;
const MISC_AT: usize = 136;
const ARRAY_AT: usize = 140;

const FLAG_MIPMAPS: u32 = 0x2_0000;
const PF_ALPHA_PIXELS: u32 = 0x1;
const PF_ALPHA_ONLY: u32 = 0x2;
const PF_FOURCC: u32 = 0x4;
const PF_LUMINANCE: u32 = 0x2_0000;
const CAPS2_CUBE: u32 = 0x200;
const DX10_MISC_CUBE: u32 = 0x4;

/// The largest width or height accepted, as a sanity bound.
const MAX_EXTENT: u32 = 1 << 15;
/// The most mip levels a texture of [`MAX_EXTENT`] can have.
const MAX_MIPS: u32 = 16;

/// A decoded `.dds` file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dds {
    /// Width of mip 0 in pixels.
    pub width: u32,
    /// Height of mip 0 in pixels.
    pub height: u32,
    /// Mip levels per face.
    pub mip_count: u32,
    /// Faces: 1, or 6 for a cube map.
    pub face_count: u32,
    /// The DXGI format of [`Dds::data`]. Legacy files map to the UNORM
    /// variant; see [`Dds::colour_space_known`].
    pub dxgi_format: u32,
    /// Whether the file says if its colour data is sRGB (only a DX10
    /// header does). Otherwise the caller decides, e.g. from the `.tobj`.
    pub colour_space_known: bool,
    /// The surfaces, face by face, mips largest first, tightly packed. A
    /// 24-bit file is widened to 32-bit BGRX here.
    pub data: Vec<u8>,
}

/// DXGI format numbers used here.
mod dxgi {
    pub const R32G32B32A32_FLOAT: u32 = 2;
    pub const R8G8B8A8_UNORM: u32 = 28;
    pub const R8_UNORM: u32 = 61;
    pub const A8_UNORM: u32 = 65;
    pub const BC1_UNORM: u32 = 71;
    pub const BC2_UNORM: u32 = 74;
    pub const BC3_UNORM: u32 = 77;
    pub const BC4_UNORM: u32 = 80;
    pub const BC5_UNORM: u32 = 83;
    pub const B8G8R8A8_UNORM: u32 = 87;
    pub const B8G8R8X8_UNORM: u32 = 88;
}

/// Reads a `.dds` file. Errors name what is wrong; never panics.
pub fn parse(bytes: &[u8]) -> Result<Dds, FormatError> {
    let r = Fields(bytes);
    if bytes.get(..4) != Some(MAGIC) {
        return Err(invalid("no DDS magic"));
    }
    if r.u32_at(4)? as usize != HEADER_LEN {
        return Err(invalid("header size is not 124"));
    }
    let width = r.u32_at(WIDTH_AT)?;
    let height = r.u32_at(HEIGHT_AT)?;
    if width == 0 || height == 0 || width > MAX_EXTENT || height > MAX_EXTENT {
        return Err(invalid(format!("size {width}x{height}")));
    }
    let mip_count = if r.u32_at(FLAGS_AT)? & FLAG_MIPMAPS != 0 {
        r.u32_at(MIPS_AT)?.max(1)
    } else {
        1
    };
    if mip_count > MAX_MIPS {
        return Err(invalid(format!("{mip_count} mip levels")));
    }
    let pf_flags = r.u32_at(PF_FLAGS_AT)?;
    let fourcc = bytes.get(FOURCC_AT..FOURCC_AT + 4).unwrap_or_default();
    let legacy_cube = r.u32_at(CAPS2_AT)? & CAPS2_CUBE != 0;

    let mut start = 4 + HEADER_LEN;
    let mut widen_24 = false;
    let (dxgi_format, colour_space_known, face_count) = if pf_flags & PF_FOURCC != 0 {
        match fourcc {
            b"DX10" => {
                start += DX10_LEN;
                let array = r.u32_at(ARRAY_AT)?.max(1);
                let cube = r.u32_at(MISC_AT)? & DX10_MISC_CUBE != 0;
                if array != 1 {
                    return Err(invalid(format!("texture array of {array}")));
                }
                (r.u32_at(DXGI_AT)?, true, if cube { 6 } else { 1 })
            }
            b"DXT1" => (dxgi::BC1_UNORM, false, faces(legacy_cube)),
            b"DXT2" | b"DXT3" => (dxgi::BC2_UNORM, false, faces(legacy_cube)),
            b"DXT4" | b"DXT5" => (dxgi::BC3_UNORM, false, faces(legacy_cube)),
            b"ATI1" | b"BC4U" => (dxgi::BC4_UNORM, true, faces(legacy_cube)),
            b"ATI2" | b"BC5U" => (dxgi::BC5_UNORM, true, faces(legacy_cube)),
            // D3DFMT_A32B32G32R32F: four floats in R, G, B, A order; linear.
            [0x74, 0, 0, 0] => (dxgi::R32G32B32A32_FLOAT, true, faces(legacy_cube)),
            other => return Err(invalid(format!("unsupported FourCC {:02x?}", other))),
        }
    } else {
        let bits = r.u32_at(BITS_AT)?;
        let red = r.u32_at(R_MASK_AT)?;
        let alpha = pf_flags & PF_ALPHA_PIXELS != 0;
        let format = match (bits, red) {
            (32, 0x00ff_0000) if alpha => dxgi::B8G8R8A8_UNORM,
            (32, 0x00ff_0000) => dxgi::B8G8R8X8_UNORM,
            (32, 0x0000_00ff) => dxgi::R8G8B8A8_UNORM,
            (24, 0x00ff_0000) => {
                widen_24 = true;
                dxgi::B8G8R8X8_UNORM
            }
            (8, _) if pf_flags & PF_LUMINANCE != 0 => dxgi::R8_UNORM,
            (8, _) if pf_flags & PF_ALPHA_ONLY != 0 => dxgi::A8_UNORM,
            _ => {
                return Err(invalid(format!(
                    "unsupported uncompressed layout ({bits}-bit, red mask {red:#x})"
                )))
            }
        };
        (format, false, faces(legacy_cube))
    };

    let payload = bytes
        .get(start..)
        .ok_or_else(|| invalid("no surface data"))?;
    let data = if widen_24 {
        let pixels = payload.len() / 3;
        let mut out = Vec::with_capacity(pixels * 4);
        for pixel in payload.chunks_exact(3) {
            out.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
        }
        out
    } else {
        payload.to_vec()
    };
    Ok(Dds {
        width,
        height,
        mip_count,
        face_count,
        dxgi_format,
        colour_space_known,
        data,
    })
}

/// Faces of a legacy file: a cube needs all six (partial cubes are rare
/// and not supported).
fn faces(cube: bool) -> u32 {
    if cube {
        6
    } else {
        1
    }
}

fn invalid(message: impl Into<String>) -> FormatError {
    FormatError::new("dds", message)
}

/// Little-endian `u32` fields of a file, bounds-checked.
struct Fields<'a>(&'a [u8]);

impl Fields<'_> {
    fn u32_at(&self, at: usize) -> Result<u32, FormatError> {
        self.0
            .get(at..at + 4)
            .and_then(|b| b.try_into().ok())
            .map(u32::from_le_bytes)
            .ok_or_else(|| invalid(format!("truncated before {at:#x}")))
    }
}

/// The sRGB twin of a UNORM DXGI format, for a legacy file the `.tobj`
/// marks as colour data; other formats are returned unchanged.
pub fn srgb_variant(dxgi_format: u32) -> u32 {
    match dxgi_format {
        dxgi::BC1_UNORM => 72,
        dxgi::BC2_UNORM => 75,
        dxgi::BC3_UNORM => 78,
        dxgi::R8G8B8A8_UNORM => 29,
        dxgi::B8G8R8A8_UNORM => 91,
        dxgi::B8G8R8X8_UNORM => 93,
        other => other,
    }
}

/// Whether a DXGI format holds sRGB colour data.
pub fn is_srgb(dxgi_format: u32) -> bool {
    matches!(dxgi_format, 29 | 72 | 75 | 78 | 91 | 93 | 99)
}

/// A `.dds` file with a DX10 header naming `dxgi_format` exactly, of
/// `face_count` faces (1, or 6 for a cube) of `mip_count` mips each;
/// `data` is the surfaces face by face, mips largest first, tightly packed.
/// The fields are Microsoft's published DDS layout.
pub fn write_dx10(
    width: u32,
    height: u32,
    mip_count: u32,
    face_count: u32,
    dxgi_format: u32,
    data: &[u8],
) -> Vec<u8> {
    const CAPS_HEIGHT_WIDTH_PIXELFORMAT: u32 = 0x1 | 0x2 | 0x4 | 0x1000;
    const CAPS_COMPLEX: u32 = 0x8;
    const CAPS_TEXTURE: u32 = 0x1000;
    const CAPS_MIPMAP: u32 = 0x40_0000;
    const CAPS2_ALL_FACES: u32 = 0xfc00;
    const PIXEL_FORMAT_AT: usize = 76;
    const CAPS_AT: usize = 108;
    const DIMENSION_AT: usize = 132;
    const TEXTURE_2D: u32 = 3;

    let mut b = vec![0u8; 4 + HEADER_LEN + DX10_LEN];
    let mut put = |at: usize, v: u32| b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    let mips = mip_count > 1;
    let cube = face_count == 6;
    put(4, HEADER_LEN as u32);
    put(
        FLAGS_AT,
        CAPS_HEIGHT_WIDTH_PIXELFORMAT | if mips { FLAG_MIPMAPS } else { 0 },
    );
    put(HEIGHT_AT, height);
    put(WIDTH_AT, width);
    put(MIPS_AT, mip_count.max(1));
    put(PIXEL_FORMAT_AT, 32);
    put(PF_FLAGS_AT, PF_FOURCC);
    put(
        CAPS_AT,
        CAPS_TEXTURE
            | if mips { CAPS_MIPMAP | CAPS_COMPLEX } else { 0 }
            | if cube { CAPS_COMPLEX } else { 0 },
    );
    put(
        CAPS2_AT,
        if cube {
            CAPS2_CUBE | CAPS2_ALL_FACES
        } else {
            0
        },
    );
    put(DXGI_AT, dxgi_format);
    put(DIMENSION_AT, TEXTURE_2D);
    put(MISC_AT, if cube { DX10_MISC_CUBE } else { 0 });
    put(ARRAY_AT, 1);
    b[..4].copy_from_slice(MAGIC);
    b[FOURCC_AT..FOURCC_AT + 4].copy_from_slice(b"DX10");
    b.extend_from_slice(data);
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A DDS header of `width`x`height` with `mips` levels, the pixel
    /// format given as (flags, FourCC, bits, red mask), then `data`.
    fn dds(
        width: u32,
        height: u32,
        mips: u32,
        pf: (u32, &[u8; 4], u32, u32),
        caps2: u32,
        dx10: Option<(u32, u32)>,
        data: &[u8],
    ) -> Vec<u8> {
        let mut b = vec![0u8; 4 + HEADER_LEN];
        b[..4].copy_from_slice(MAGIC);
        let put =
            |b: &mut Vec<u8>, at: usize, v: u32| b[at..at + 4].copy_from_slice(&v.to_le_bytes());
        put(&mut b, 4, 124);
        put(&mut b, FLAGS_AT, if mips > 1 { FLAG_MIPMAPS } else { 0 });
        put(&mut b, HEIGHT_AT, height);
        put(&mut b, WIDTH_AT, width);
        put(&mut b, MIPS_AT, mips);
        put(&mut b, PF_FLAGS_AT, pf.0);
        b[FOURCC_AT..FOURCC_AT + 4].copy_from_slice(pf.1);
        put(&mut b, BITS_AT, pf.2);
        put(&mut b, R_MASK_AT, pf.3);
        put(&mut b, CAPS2_AT, caps2);
        if let Some((format, misc)) = dx10 {
            b.extend([0u8; DX10_LEN]);
            put(&mut b, DXGI_AT, format);
            put(&mut b, MISC_AT, misc);
            put(&mut b, ARRAY_AT, 1);
        }
        b.extend_from_slice(data);
        b
    }

    #[test]
    fn reads_a_dx10_file_with_its_colour_space() {
        let file = dds(
            8,
            4,
            2,
            (PF_FOURCC, b"DX10", 0, 0),
            0,
            Some((78, 0)),
            &[7; 48],
        );
        let parsed = parse(&file).unwrap();
        assert_eq!(
            (
                parsed.width,
                parsed.height,
                parsed.mip_count,
                parsed.face_count
            ),
            (8, 4, 2, 1)
        );
        assert_eq!(parsed.dxgi_format, 78);
        assert!(parsed.colour_space_known);
        assert_eq!(parsed.data, [7; 48]);
        let cube = dds(
            4,
            4,
            1,
            (PF_FOURCC, b"DX10", 0, 0),
            0,
            Some((72, DX10_MISC_CUBE)),
            &[],
        );
        assert_eq!(parse(&cube).unwrap().face_count, 6);
    }

    #[test]
    fn maps_legacy_fourccs_and_leaves_the_colour_space_open() {
        let dxt5 = parse(&dds(4, 4, 1, (PF_FOURCC, b"DXT5", 0, 0), 0, None, &[1; 16])).unwrap();
        assert_eq!(dxt5.dxgi_format, dxgi::BC3_UNORM);
        assert!(!dxt5.colour_space_known);
        assert_eq!(srgb_variant(dxt5.dxgi_format), 78);
        let ati2 = parse(&dds(4, 4, 1, (PF_FOURCC, b"ATI2", 0, 0), 0, None, &[])).unwrap();
        assert_eq!(ati2.dxgi_format, dxgi::BC5_UNORM);
        assert!(ati2.colour_space_known);
        let cube = parse(&dds(
            4,
            4,
            1,
            (PF_FOURCC, b"DXT1", 0, 0),
            CAPS2_CUBE,
            None,
            &[],
        ))
        .unwrap();
        assert_eq!(cube.face_count, 6);
    }

    /// Legacy FourCC 0x74 (D3DFMT_A32B32G32R32F, 17 mod files in
    /// `tobj-loose.txt` §3) is DXGI R32G32B32A32_FLOAT, linear data.
    #[test]
    fn reads_legacy_32_bit_float_files() {
        let pixel: Vec<u8> = [1.0f32, 0.5, -2.0, 4.0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let float = parse(&dds(
            1,
            1,
            1,
            (PF_FOURCC, &[0x74, 0, 0, 0], 0, 0),
            0,
            None,
            &pixel,
        ))
        .unwrap();
        assert_eq!(float.dxgi_format, dxgi::R32G32B32A32_FLOAT);
        assert!(float.colour_space_known);
        assert_eq!(float.data, pixel);
    }

    /// What `write_dx10` makes reads back with the same format, faces and
    /// bytes.
    #[test]
    fn writes_dx10_files_that_read_back() {
        let flat = write_dx10(8, 4, 2, 1, 78, &[3; 48]);
        let parsed = parse(&flat).unwrap();
        assert_eq!(
            (
                parsed.width,
                parsed.height,
                parsed.mip_count,
                parsed.face_count
            ),
            (8, 4, 2, 1)
        );
        assert_eq!((parsed.dxgi_format, parsed.colour_space_known), (78, true));
        assert_eq!(parsed.data, [3; 48]);
        let cube = parse(&write_dx10(4, 4, 1, 6, 91, &[1; 384])).unwrap();
        assert_eq!((cube.face_count, cube.dxgi_format), (6, 91));
        assert!(is_srgb(91) && is_srgb(78) && !is_srgb(77) && !is_srgb(83));
    }

    #[test]
    fn reads_uncompressed_layouts() {
        let bgra = dds(
            1,
            1,
            1,
            (PF_ALPHA_PIXELS, &[0; 4], 32, 0xff_0000),
            0,
            None,
            &[1, 2, 3, 4],
        );
        assert_eq!(parse(&bgra).unwrap().dxgi_format, dxgi::B8G8R8A8_UNORM);
        let rgba = dds(
            1,
            1,
            1,
            (PF_ALPHA_PIXELS, &[0; 4], 32, 0xff),
            0,
            None,
            &[1, 2, 3, 4],
        );
        assert_eq!(parse(&rgba).unwrap().dxgi_format, dxgi::R8G8B8A8_UNORM);
        let bgr = parse(&dds(
            2,
            1,
            1,
            (0, &[0; 4], 24, 0xff_0000),
            0,
            None,
            &[1, 2, 3, 4, 5, 6],
        ))
        .unwrap();
        assert_eq!(bgr.dxgi_format, dxgi::B8G8R8X8_UNORM);
        assert_eq!(bgr.data, [1, 2, 3, 255, 4, 5, 6, 255]);
        let lum = dds(1, 1, 1, (PF_LUMINANCE, &[0; 4], 8, 0xff), 0, None, &[9]);
        assert_eq!(parse(&lum).unwrap().dxgi_format, dxgi::R8_UNORM);
    }

    #[test]
    fn rejects_bad_files_without_panicking() {
        assert!(parse(b"").is_err());
        assert!(parse(b"DDS ").is_err());
        let unknown = dds(1, 1, 1, (PF_FOURCC, b"XYZW", 0, 0), 0, None, &[]);
        assert!(parse(&unknown).is_err());
        let huge = dds(1 << 20, 1, 1, (PF_FOURCC, b"DXT1", 0, 0), 0, None, &[]);
        assert!(parse(&huge).is_err());
        let mut truncated = dds(4, 4, 1, (PF_FOURCC, b"DX10", 0, 0), 0, Some((78, 0)), &[]);
        truncated.truncate(130);
        assert!(parse(&truncated).is_err());
        for cut in 0..140 {
            let file = dds(
                4,
                4,
                1,
                (PF_FOURCC, b"DX10", 0, 0),
                0,
                Some((78, 0)),
                &[0; 16],
            );
            let _ = parse(&file[..cut]);
        }
    }
}
