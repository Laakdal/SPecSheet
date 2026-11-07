//! Block-compressed texels (BC1 to BC5) decoded to RGBA8, for consumers that
//! cannot sample compressed data, such as UI images. Written from
//! Microsoft's published description of the formats ("Texture Block
//! Compression in Direct3D 11"; BC1 to BC3 as DXT1, DXT3, DXT5).
//!
//! A block covers 4x4 texels in row-major order. Blocks are stored row by
//! row, `ceil(width / 4)` per row; texels past the image edge are decoded and
//! dropped. Values come out in the file's own space: sRGB data stays sRGB.
//! BC4 lands in red and BC5 in red and green, as a GPU samples them (the
//! other colour channels 0, alpha 255).

use crate::FormatError;

/// A block-compressed format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BcFormat {
    /// BC1 (DXT1): RGB565 endpoints, 1-bit alpha.
    Bc1,
    /// BC2 (DXT3): BC1 colour plus explicit 4-bit alpha.
    Bc2,
    /// BC3 (DXT5): BC1 colour plus interpolated alpha.
    Bc3,
    /// BC4: one interpolated channel (unsigned).
    Bc4,
    /// BC5: two interpolated channels (unsigned).
    Bc5,
}

impl BcFormat {
    /// Bytes per 4x4 block.
    pub fn block_bytes(self) -> usize {
        match self {
            Self::Bc1 | Self::Bc4 => 8,
            Self::Bc2 | Self::Bc3 | Self::Bc5 => 16,
        }
    }
}

const WHAT: &str = "bc";

/// Decodes one surface of `width` x `height` texels to tightly packed RGBA8.
/// `data` must hold exactly the surface's blocks.
pub fn decode_rgba8(
    format: BcFormat,
    width: u32,
    height: u32,
    data: &[u8],
) -> Result<Vec<u8>, FormatError> {
    let error = |message: String| FormatError::new(WHAT, message);
    if width == 0 || height == 0 {
        return Err(error(format!("empty surface {width}x{height}")));
    }
    let (w, h) = (width as usize, height as usize);
    let (blocks_x, blocks_y) = (w.div_ceil(4), h.div_ceil(4));
    let expected = blocks_x
        .checked_mul(blocks_y)
        .and_then(|n| n.checked_mul(format.block_bytes()))
        .ok_or_else(|| error(format!("surface {width}x{height} too large")))?;
    if data.len() != expected {
        return Err(error(format!(
            "{format:?} {width}x{height} needs {expected} bytes, got {}",
            data.len()
        )));
    }
    let out_len = w
        .checked_mul(h)
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(|| error(format!("surface {width}x{height} too large")))?;
    let mut out = vec![0u8; out_len];
    for (index, block) in data.chunks_exact(format.block_bytes()).enumerate() {
        let texels = decode_block(format, block);
        let (bx, by) = (index % blocks_x, index / blocks_x);
        for (i, texel) in texels.iter().enumerate() {
            let (x, y) = (bx * 4 + i % 4, by * 4 + i / 4);
            if x < w && y < h {
                let at = (y * w + x) * 4;
                out[at..at + 4].copy_from_slice(texel);
            }
        }
    }
    Ok(out)
}

/// The 16 texels of one block; `block` has the format's block size.
fn decode_block(format: BcFormat, block: &[u8]) -> [[u8; 4]; 16] {
    let mut texels = [[0, 0, 0, 255]; 16];
    match format {
        BcFormat::Bc1 => colour(block, true, &mut texels),
        BcFormat::Bc2 => {
            colour(&block[8..], false, &mut texels);
            let bits = u64::from_le_bytes(array(&block[..8]));
            for (i, texel) in texels.iter_mut().enumerate() {
                texel[3] = ((bits >> (4 * i)) & 0xF) as u8 * 17;
            }
        }
        BcFormat::Bc3 => {
            colour(&block[8..], false, &mut texels);
            channel(&block[..8], 3, &mut texels);
        }
        BcFormat::Bc4 => channel(block, 0, &mut texels),
        BcFormat::Bc5 => {
            channel(&block[..8], 0, &mut texels);
            channel(&block[8..], 1, &mut texels);
        }
    }
    texels
}

fn array<const N: usize>(bytes: &[u8]) -> [u8; N] {
    let mut out = [0; N];
    out.copy_from_slice(&bytes[..N]);
    out
}

/// An RGB565 value widened to 8 bits per channel by repeating its top bits.
fn rgb565(value: u16) -> [u16; 3] {
    let (r, g, b) = ((value >> 11) & 31, (value >> 5) & 63, value & 31);
    [
        (r << 3) | (r >> 2),
        (g << 2) | (g >> 4),
        (b << 3) | (b >> 2),
    ]
}

/// The 8-byte colour block. With `one_bit_alpha` (BC1) and the first
/// endpoint not above the second, the block has three colours and index 3
/// is transparent black; BC2 and BC3 always use four colours.
fn colour(block: &[u8], one_bit_alpha: bool, texels: &mut [[u8; 4]; 16]) {
    let (c0, c1) = (
        u16::from_le_bytes([block[0], block[1]]),
        u16::from_le_bytes([block[2], block[3]]),
    );
    let (e0, e1) = (rgb565(c0), rgb565(c1));
    let mix =
        |a: u16, wa: u16, b: u16, wb: u16| ((a * wa + b * wb + (wa + wb) / 2) / (wa + wb)) as u8;
    let mut palette = [[0u8; 4]; 4];
    palette[0] = [e0[0] as u8, e0[1] as u8, e0[2] as u8, 255];
    palette[1] = [e1[0] as u8, e1[1] as u8, e1[2] as u8, 255];
    if c0 > c1 || !one_bit_alpha {
        for c in 0..3 {
            palette[2][c] = mix(e0[c], 2, e1[c], 1);
            palette[3][c] = mix(e0[c], 1, e1[c], 2);
        }
        palette[2][3] = 255;
        palette[3][3] = 255;
    } else {
        for c in 0..3 {
            palette[2][c] = mix(e0[c], 1, e1[c], 1);
        }
        palette[2][3] = 255;
        palette[3] = [0, 0, 0, 0];
    }
    let indices = u32::from_le_bytes(array(&block[4..8]));
    for (i, texel) in texels.iter_mut().enumerate() {
        *texel = palette[((indices >> (2 * i)) & 3) as usize];
    }
}

/// An 8-byte interpolated channel block (BC3 alpha, BC4, BC5) into channel
/// `c`. With the first endpoint above the second there are eight values;
/// otherwise six, and indices 6 and 7 are 0 and 255.
fn channel(block: &[u8], c: usize, texels: &mut [[u8; 4]; 16]) {
    let (a0, a1) = (u16::from(block[0]), u16::from(block[1]));
    let mut values = [0u8; 8];
    values[0] = a0 as u8;
    values[1] = a1 as u8;
    let steps: u16 = if a0 > a1 { 7 } else { 5 };
    for i in 1..steps {
        let value = (a0 * (steps - i) + a1 * i + steps / 2) / steps;
        values[1 + i as usize] = value as u8;
    }
    if a0 <= a1 {
        values[6] = 0;
        values[7] = 255;
    }
    let mut bits = [0u8; 8];
    bits[..6].copy_from_slice(&block[2..8]);
    let bits = u64::from_le_bytes(bits);
    for (i, texel) in texels.iter_mut().enumerate() {
        texel[c] = values[((bits >> (3 * i)) & 7) as usize];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A BC1 block from its endpoints and the 16 two-bit indices.
    fn bc1(c0: u16, c1: u16, indices: [u8; 16]) -> Vec<u8> {
        let bits = indices
            .iter()
            .enumerate()
            .fold(0u32, |acc, (i, &v)| acc | u32::from(v) << (2 * i));
        [c0.to_le_bytes(), c1.to_le_bytes()]
            .concat()
            .into_iter()
            .chain(bits.to_le_bytes())
            .collect()
    }

    /// An interpolated channel block from its endpoints and 16 three-bit
    /// indices.
    fn channel_block(a0: u8, a1: u8, indices: [u8; 16]) -> Vec<u8> {
        let bits = indices
            .iter()
            .enumerate()
            .fold(0u64, |acc, (i, &v)| acc | u64::from(v) << (3 * i));
        let mut out = vec![a0, a1];
        out.extend_from_slice(&bits.to_le_bytes()[..6]);
        out
    }

    fn texel(rgba: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
        let at = (y * width + x) * 4;
        rgba[at..at + 4].try_into().expect("texel")
    }

    const RED: u16 = 0xF800;
    const BLUE: u16 = 0x001F;

    #[test]
    fn bc1_four_colour_block_interpolates_thirds() {
        let mut indices = [0u8; 16];
        indices[1] = 1;
        indices[2] = 2;
        indices[3] = 3;
        let rgba = decode_rgba8(BcFormat::Bc1, 4, 4, &bc1(RED, BLUE, indices)).expect("decodes");
        assert_eq!(texel(&rgba, 4, 0, 0), [255, 0, 0, 255]);
        assert_eq!(texel(&rgba, 4, 1, 0), [0, 0, 255, 255]);
        assert_eq!(texel(&rgba, 4, 2, 0), [170, 0, 85, 255]);
        assert_eq!(texel(&rgba, 4, 3, 0), [85, 0, 170, 255]);
        assert_eq!(texel(&rgba, 4, 3, 3), [255, 0, 0, 255]);
    }

    #[test]
    fn bc1_three_colour_block_has_transparent_black() {
        // First endpoint not above the second: three colours.
        let mut indices = [2u8; 16];
        indices[15] = 3;
        let rgba = decode_rgba8(BcFormat::Bc1, 4, 4, &bc1(BLUE, RED, indices)).expect("decodes");
        assert_eq!(texel(&rgba, 4, 0, 0), [128, 0, 128, 255]);
        assert_eq!(texel(&rgba, 4, 3, 3), [0, 0, 0, 0]);
    }

    #[test]
    fn rgb565_widens_by_repeating_the_top_bits() {
        assert_eq!(rgb565(0xFFFF), [255, 255, 255]);
        assert_eq!(rgb565(0), [0, 0, 0]);
        // r = 16, g = 32, b = 16: the midpoints.
        assert_eq!(rgb565(16 << 11 | 32 << 5 | 16), [132, 130, 132]);
    }

    #[test]
    fn bc2_alpha_is_explicit_and_colour_always_four_entries() {
        let alpha: u64 = (0..16).fold(0, |acc, i| acc | (i as u64) << (4 * i));
        let mut block = alpha.to_le_bytes().to_vec();
        // Endpoints in three-colour order: BC2 still reads index 3 as a colour.
        block.extend(bc1(BLUE, RED, [3; 16]));
        let rgba = decode_rgba8(BcFormat::Bc2, 4, 4, &block).expect("decodes");
        assert_eq!(texel(&rgba, 4, 0, 0), [170, 0, 85, 0]);
        assert_eq!(texel(&rgba, 4, 1, 0)[3], 17);
        assert_eq!(texel(&rgba, 4, 3, 3)[3], 255);
    }

    #[test]
    fn bc3_alpha_has_eight_or_six_values() {
        // a0 > a1: index 2 is (6 a0 + a1) / 7, index 7 is (a0 + 6 a1) / 7.
        let mut indices = [0u8; 16];
        indices[1] = 1;
        indices[2] = 2;
        indices[3] = 7;
        let mut block = channel_block(255, 0, indices);
        block.extend(bc1(0xFFFF, 0, [0; 16]));
        let rgba = decode_rgba8(BcFormat::Bc3, 4, 4, &block).expect("decodes");
        let alpha: Vec<u8> = (0..4).map(|x| texel(&rgba, 4, x, 0)[3]).collect();
        assert_eq!(alpha, [255, 0, 219, 36]);
        assert_eq!(texel(&rgba, 4, 0, 0), [255, 255, 255, 255]);
        // a0 <= a1: index 2 is (4 a0 + a1) / 5, and 6 and 7 are 0 and 255.
        let indices = [2, 5, 6, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let mut block = channel_block(0, 100, indices);
        block.extend(bc1(0xFFFF, 0, [0; 16]));
        let rgba = decode_rgba8(BcFormat::Bc3, 4, 4, &block).expect("decodes");
        let alpha: Vec<u8> = (0..4).map(|x| texel(&rgba, 4, x, 0)[3]).collect();
        assert_eq!(alpha, [20, 80, 0, 255]);
    }

    #[test]
    fn bc4_is_red_and_bc5_red_and_green() {
        let rgba =
            decode_rgba8(BcFormat::Bc4, 4, 4, &channel_block(200, 10, [0; 16])).expect("bc4");
        assert_eq!(texel(&rgba, 4, 2, 2), [200, 0, 0, 255]);
        let mut block = channel_block(200, 10, [0; 16]);
        block.extend(channel_block(10, 200, [1; 16]));
        let rgba = decode_rgba8(BcFormat::Bc5, 4, 4, &block).expect("bc5");
        assert_eq!(texel(&rgba, 4, 1, 3), [200, 200, 0, 255]);
    }

    #[test]
    fn small_and_odd_sizes_crop_their_blocks() {
        let mut indices = [1u8; 16];
        indices[0] = 0;
        let block = bc1(RED, BLUE, indices);
        let one = decode_rgba8(BcFormat::Bc1, 1, 1, &block).expect("1x1");
        assert_eq!(one, [255, 0, 0, 255]);
        let two = decode_rgba8(BcFormat::Bc1, 2, 2, &block).expect("2x2");
        assert_eq!(two.len(), 16);
        assert_eq!(texel(&two, 2, 1, 1), [0, 0, 255, 255]);
        // 5x3: two blocks across, one down; the second block's first column only.
        let mut data = bc1(BLUE, BLUE, [0; 16]);
        data.extend(bc1(RED, RED, [0; 16]));
        let wide = decode_rgba8(BcFormat::Bc1, 5, 3, &data).expect("5x3");
        assert_eq!(wide.len(), 5 * 3 * 4);
        assert_eq!(texel(&wide, 5, 3, 2), [0, 0, 255, 255]);
        assert_eq!(texel(&wide, 5, 4, 2), [255, 0, 0, 255]);
    }

    #[test]
    fn wrong_lengths_and_empty_surfaces_are_errors() {
        let block = bc1(RED, BLUE, [0; 16]);
        assert!(decode_rgba8(BcFormat::Bc1, 4, 4, &block[..7]).is_err());
        assert!(decode_rgba8(
            BcFormat::Bc1,
            4,
            4,
            &[block.clone(), block.clone()].concat()
        )
        .is_err());
        assert!(decode_rgba8(BcFormat::Bc3, 4, 4, &block).is_err());
        assert!(decode_rgba8(BcFormat::Bc1, 5, 4, &block).is_err());
        assert!(decode_rgba8(BcFormat::Bc1, 0, 4, &[]).is_err());
        assert!(decode_rgba8(BcFormat::Bc5, u32::MAX, u32::MAX, &[]).is_err());
    }
}
