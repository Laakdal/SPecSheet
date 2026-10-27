//! PMG model geometry (version 0x15, signature `\x15gmP`): parts, locators
//! and pieces (meshes with interleaved vertex streams and `u16` triangle
//! lists). Layout evidence: `docs/formats/models.md#pmg-geometry-version-0x15-signature-x15gmp`.

use crate::binary::{FormatError, Reader};

const VERSION: u8 = 0x15;
const SIGNATURE: &[u8; 3] = b"gmP";
const WHAT: &str = "pmg";
/// Offsets of absent vertex streams.
const ABSENT: i32 = -1;
const PIECE_HEADER_LEN: usize = 100;
const LOCATOR_LEN: usize = 44;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Aabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Aabb {
    pub fn center(&self) -> [f32; 3] {
        [0, 1, 2].map(|i| (self.min[i] + self.max[i]) / 2.0)
    }

    pub fn union(&self, other: &Aabb) -> Aabb {
        Aabb {
            min: [0, 1, 2].map(|i| self.min[i].min(other.min[i])),
            max: [0, 1, 2].map(|i| self.max[i].max(other.max[i])),
        }
    }

    /// The box shrunk by `margin` on every side (never inverted).
    pub fn shrunk(&self, margin: f32) -> Aabb {
        let center = self.center();
        Aabb {
            min: [0, 1, 2].map(|i| (self.min[i] + margin).min(center[i])),
            max: [0, 1, 2].map(|i| (self.max[i] - margin).max(center[i])),
        }
    }

    pub fn contains_point(&self, p: [f32; 3]) -> bool {
        (0..3).all(|i| self.min[i] <= p[i] && p[i] <= self.max[i])
    }

    pub fn contains_box(&self, other: &Aabb) -> bool {
        self.contains_point(other.min) && self.contains_point(other.max)
    }
}

/// A named group of pieces and locators; PMD variants toggle parts.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub name: String,
    pub pieces: std::ops::Range<usize>,
    pub locators: std::ops::Range<usize>,
}

/// A named transform inside the model, used e.g. as an attachment point.
#[derive(Clone, Debug, PartialEq)]
pub struct Locator {
    pub name: String,
    pub position: [f32; 3],
    pub scale: f32,
    /// Rotation quaternion stored `(w, x, y, z)`.
    pub rotation_wxyz: [f32; 4],
    /// Hookup string from the string pool (e.g. a light or effect id).
    /// Locators without one point at an empty string.
    pub hookup: Option<String>,
}

/// One mesh drawn with a single material.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    /// Index into the PMD look's material list.
    pub material: u32,
    pub aabb: Aabb,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// `(x, y, z, w)` tangents, if present.
    pub tangents: Option<Vec<[f32; 4]>>,
    /// Vertex colour, packed RGBA8.
    pub colors: Vec<u32>,
    /// One `Vec` per UV set, each with one entry per vertex.
    pub uv_sets: Vec<Vec<[f32; 2]>>,
    /// Triangle list indices into this piece's vertices.
    pub indices: Vec<u16>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pmg {
    pub aabb: Aabb,
    pub parts: Vec<Part>,
    pub locators: Vec<Locator>,
    pub pieces: Vec<Piece>,
    pub bone_count: u32,
}

struct PieceHeader {
    index_count: usize,
    vertex_count: usize,
    uv_set_count: usize,
    material: u32,
    aabb: Aabb,
    /// Bytes per interleaved vertex, as stored in the file.
    stride: usize,
    position_offset: i32,
    normal_offset: i32,
    uv_offset: i32,
    color_offset: i32,
    color2_offset: i32,
    tangent_offset: i32,
    bone_index_offset: i32,
    bone_weight_offset: i32,
    index_offset: i32,
}

impl Pmg {
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        let mut r = Reader::new(WHAT, bytes);
        let version = r.u8()?;
        if r.bytes(3)? != SIGNATURE {
            return Err(r.error("missing `gmP` signature"));
        }
        if version != VERSION {
            return Err(r.error(format!("unsupported version {version:#04x}")));
        }
        let piece_count = r.count(PIECE_HEADER_LEN)?;
        let part_count = r.count(24)?;
        let bone_count = r.u32()?;
        let _weight_width = r.u32()?;
        let locator_count = r.count(LOCATOR_LEN)?;
        let _skeleton_hash = r.u64()?;
        let _center = r.vec3()?;
        let _radius = r.f32()?;
        let aabb = Aabb {
            min: r.vec3()?,
            max: r.vec3()?,
        };
        let _skeleton_offset = r.u32()?;
        let parts_offset = r.u32()? as usize;
        let locators_offset = r.u32()? as usize;
        let pieces_offset = r.u32()? as usize;
        let string_pool_offset = r.u32()? as usize;
        let string_pool_size = r.u32()? as usize;

        r.seek(parts_offset)?;
        let mut parts = Vec::with_capacity(part_count);
        for _ in 0..part_count {
            let name = r.token()?.decode();
            let (piece_n, piece_start) = (r.u32()? as usize, r.u32()? as usize);
            let (locator_n, locator_start) = (r.u32()? as usize, r.u32()? as usize);
            let pieces = range(&r, piece_n, piece_start, piece_count, "piece")?;
            let locators = range(&r, locator_n, locator_start, locator_count, "locator")?;
            parts.push(Part {
                name,
                pieces,
                locators,
            });
        }

        let string_pool = string_pool_offset..string_pool_offset + string_pool_size;
        r.seek(locators_offset)?;
        let mut locators = Vec::with_capacity(locator_count);
        for _ in 0..locator_count {
            let name = r.token()?.decode();
            let position = r.vec3()?;
            let scale = r.f32()?;
            let rotation_wxyz = r.vec4()?;
            let hookup_offset = r.i32()?;
            let hookup = if hookup_offset < 0 {
                None
            } else {
                let at = string_pool.start + hookup_offset as usize;
                if !string_pool.contains(&at) {
                    return Err(r.error(format!("locator `{name}` hookup outside string pool")));
                }
                Some(r.c_string_at(at)?).filter(|s| !s.is_empty())
            };
            locators.push(Locator {
                name,
                position,
                scale,
                rotation_wxyz,
                hookup,
            });
        }

        r.seek(pieces_offset)?;
        let headers = (0..piece_count)
            .map(|_| read_piece_header(&mut r))
            .collect::<Result<Vec<_>, _>>()?;
        let pieces = headers
            .iter()
            .map(|h| read_piece(&mut r, h))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            aabb,
            parts,
            locators,
            pieces,
            bone_count,
        })
    }

    pub fn part(&self, name: &str) -> Option<&Part> {
        self.parts.iter().find(|p| p.name == name)
    }
}

/// `(count, start)` as stored, validated against `total`.
fn range(
    r: &Reader<'_>,
    count: usize,
    start: usize,
    total: usize,
    what: &str,
) -> Result<std::ops::Range<usize>, FormatError> {
    match start.checked_add(count) {
        Some(end) if end <= total => Ok(start..end),
        _ => Err(r.error(format!("{what} range {start}+{count} exceeds {total}"))),
    }
}

fn read_piece_header(r: &mut Reader<'_>) -> Result<PieceHeader, FormatError> {
    let index_count = r.count(2)?;
    let vertex_count = r.count(4)?;
    let _uv_mask = r.u32()?;
    let uv_set_count = r.u32()? as usize;
    let material = r.u32()?;
    let _center = r.vec3()?;
    let _radius = r.f32()?;
    let aabb = Aabb {
        min: r.vec3()?,
        max: r.vec3()?,
    };
    let stride = r.u32()? as usize;
    let header = PieceHeader {
        index_count,
        vertex_count,
        uv_set_count,
        material,
        aabb,
        stride,
        position_offset: r.i32()?,
        normal_offset: r.i32()?,
        uv_offset: r.i32()?,
        color_offset: r.i32()?,
        color2_offset: r.i32()?,
        tangent_offset: r.i32()?,
        bone_index_offset: r.i32()?,
        bone_weight_offset: r.i32()?,
        index_offset: r.i32()?,
    };
    if uv_set_count > 8 {
        return Err(r.error(format!("implausible UV set count {uv_set_count}")));
    }
    if index_count % 3 != 0 {
        return Err(r.error(format!("index count {index_count} is not a triangle list")));
    }
    Ok(header)
}

/// Reads one piece. Vertex attributes are interleaved; the stride is the
/// sum of the present attribute sizes, and each stream starts at its own
/// recorded offset.
fn read_piece(r: &mut Reader<'_>, h: &PieceHeader) -> Result<Piece, FormatError> {
    let present = |offset: i32| offset != ABSENT;
    let stride = 12
        + 12
        + if present(h.tangent_offset) { 16 } else { 0 }
        + if present(h.color_offset) { 4 } else { 0 }
        + if present(h.color2_offset) { 4 } else { 0 }
        + if present(h.uv_offset) {
            8 * h.uv_set_count
        } else {
            0
        }
        + if present(h.bone_index_offset) { 4 } else { 0 }
        + if present(h.bone_weight_offset) { 4 } else { 0 };
    if stride != h.stride {
        return Err(r.error(format!(
            "vertex stride {} does not match the present streams ({stride} bytes)",
            h.stride
        )));
    }

    let stream = |r: &mut Reader<'_>, offset: i32, what: &str| -> Result<usize, FormatError> {
        let offset =
            usize::try_from(offset).map_err(|_| r.error(format!("missing {what} stream")))?;
        let last = (h.vertex_count.max(1) - 1) * stride;
        if offset + last > r.len() {
            return Err(r.error(format!("{what} stream exceeds file")));
        }
        Ok(offset)
    };

    let mut positions = Vec::with_capacity(h.vertex_count);
    let mut normals = Vec::with_capacity(h.vertex_count);
    let base = stream(r, h.position_offset, "position")?;
    let normal_base = stream(r, h.normal_offset, "normal")?;
    for i in 0..h.vertex_count {
        r.seek(base + i * stride)?;
        positions.push(r.vec3()?);
        r.seek(normal_base + i * stride)?;
        normals.push(r.vec3()?);
    }

    let tangents = if present(h.tangent_offset) {
        let base = stream(r, h.tangent_offset, "tangent")?;
        let mut out = Vec::with_capacity(h.vertex_count);
        for i in 0..h.vertex_count {
            r.seek(base + i * stride)?;
            out.push(r.vec4()?);
        }
        Some(out)
    } else {
        None
    };

    let mut colors = Vec::with_capacity(h.vertex_count);
    if present(h.color_offset) {
        let base = stream(r, h.color_offset, "color")?;
        for i in 0..h.vertex_count {
            r.seek(base + i * stride)?;
            colors.push(r.u32()?);
        }
    }

    let mut uv_sets = vec![
        Vec::with_capacity(h.vertex_count);
        if present(h.uv_offset) {
            h.uv_set_count
        } else {
            0
        }
    ];
    if present(h.uv_offset) {
        let base = stream(r, h.uv_offset, "uv")?;
        for i in 0..h.vertex_count {
            r.seek(base + i * stride)?;
            for set in &mut uv_sets {
                set.push([r.f32()?, r.f32()?]);
            }
        }
    }

    let index_offset =
        usize::try_from(h.index_offset).map_err(|_| r.error("missing index stream"))?;
    r.seek(index_offset)?;
    let mut indices = Vec::with_capacity(h.index_count);
    for _ in 0..h.index_count {
        let index = r.u16()?;
        if usize::from(index) >= h.vertex_count {
            return Err(r.error(format!("index {index} out of {} vertices", h.vertex_count)));
        }
        indices.push(index);
    }

    Ok(Piece {
        material: h.material,
        aabb: h.aabb,
        positions,
        normals,
        tangents,
        colors,
        uv_sets,
        indices,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::Token;

    /// A one-part, one-locator, one-triangle model.
    fn sample() -> Vec<u8> {
        let mut b = vec![VERSION, b'g', b'm', b'P'];
        let push_u32 = |b: &mut Vec<u8>, v: u32| b.extend(v.to_le_bytes());
        let push_f32 = |b: &mut Vec<u8>, v: f32| b.extend(v.to_le_bytes());
        for v in [1u32, 1, 0, 0, 1] {
            push_u32(&mut b, v);
        }
        b.extend(0u64.to_le_bytes());
        for _ in 0..10 {
            push_f32(&mut b, 0.0);
        }
        // Offsets: header is 0x70 bytes; part (24), locator (44), piece header (100).
        let parts = 0x70u32;
        let locators = parts + 24;
        let pieces = locators + 44;
        let strings = pieces + 100;
        let vertices = strings + 8;
        let stride = 12 + 12 + 4 + 8;
        let indices = vertices + 3 * stride;
        for v in [
            0,
            parts,
            locators,
            pieces,
            strings,
            8,
            vertices,
            3 * stride,
            indices,
            6,
        ] {
            push_u32(&mut b, v);
        }
        assert_eq!(b.len(), parts as usize);
        b.extend(Token::encode("body").expect("t").0.to_le_bytes());
        for v in [1u32, 0, 1, 0] {
            push_u32(&mut b, v);
        }
        b.extend(Token::encode("s_mirror").expect("t").0.to_le_bytes());
        for v in [1.0f32, 2.0, 3.0, 1.0, 1.0, 0.0, 0.0, 0.0] {
            push_f32(&mut b, v);
        }
        push_u32(&mut b, 0); // hookup at pool offset 0
                             // Piece header.
        for v in [3u32, 3, 0xffff_fff0, 1, 2] {
            push_u32(&mut b, v);
        }
        for _ in 0..10 {
            push_f32(&mut b, 0.0);
        }
        push_u32(&mut b, 36);
        for v in [
            vertices as i32,
            vertices as i32 + 12,
            vertices as i32 + 28,
            vertices as i32 + 24,
            -1,
            -1,
            -1,
            -1,
            indices as i32,
        ] {
            b.extend(v.to_le_bytes());
        }
        b.extend(b"horn_1\0\0");
        for i in 0..3 {
            for v in [i as f32, 0.0, 0.0, 0.0, 1.0, 0.0] {
                push_f32(&mut b, v);
            }
            push_u32(&mut b, 0xff00_00ff);
            push_f32(&mut b, 0.5);
            push_f32(&mut b, i as f32);
        }
        for v in [0u16, 1, 2] {
            b.extend(v.to_le_bytes());
        }
        b
    }

    #[test]
    fn parses_parts_locators_and_pieces() {
        let pmg = Pmg::parse(&sample()).expect("parse");
        assert_eq!(pmg.parts[0].name, "body");
        assert_eq!(pmg.parts[0].pieces, 0..1);
        let locator = &pmg.locators[0];
        assert_eq!(locator.name, "s_mirror");
        assert_eq!(locator.position, [1.0, 2.0, 3.0]);
        assert_eq!(locator.rotation_wxyz, [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(locator.hookup.as_deref(), Some("horn_1"));
        let piece = &pmg.pieces[0];
        assert_eq!(piece.material, 2);
        assert_eq!(piece.positions[2], [2.0, 0.0, 0.0]);
        assert_eq!(piece.normals[1], [0.0, 1.0, 0.0]);
        assert_eq!(
            piece.uv_sets,
            vec![vec![[0.5, 0.0], [0.5, 1.0], [0.5, 2.0]]]
        );
        assert_eq!(piece.colors, vec![0xff00_00ff; 3]);
        assert_eq!(piece.indices, [0, 1, 2]);
    }

    /// Byte offsets inside `sample()`.
    const SAMPLE_PIECE: usize = 0x70 + 24 + 44;
    const SAMPLE_STRINGS: usize = SAMPLE_PIECE + 100;

    #[test]
    fn empty_hookup_string_means_no_hookup() {
        // Locators without a hookup point at an empty string, usually
        // pool offset 0 (`research/evidence/pmg-layout-derivation.txt`).
        let mut bytes = sample();
        bytes[SAMPLE_STRINGS] = 0;
        let pmg = Pmg::parse(&bytes).expect("parse");
        assert_eq!(pmg.locators[0].hookup, None);
    }

    #[test]
    fn rejects_a_stride_that_disagrees_with_the_streams() {
        let mut bytes = sample();
        bytes[SAMPLE_PIECE + 60..SAMPLE_PIECE + 64].copy_from_slice(&40u32.to_le_bytes());
        assert!(Pmg::parse(&bytes).is_err());
    }

    #[test]
    fn rejects_out_of_range_indices() {
        let mut bytes = sample();
        let len = bytes.len();
        bytes[len - 2..].copy_from_slice(&7u16.to_le_bytes());
        assert!(Pmg::parse(&bytes).is_err());
    }

    #[test]
    fn rejects_truncated_input() {
        let bytes = sample();
        for len in [0, 3, 0x40, 0x70, bytes.len() - 1] {
            assert!(Pmg::parse(&bytes[..len]).is_err(), "len {len}");
        }
    }
}
