//! PMG model geometry (version 0x15): parts, locators and pieces (meshes
//! with interleaved vertex streams and `u16` triangle lists).
//! Format: `docs/formats/models.md`.

use std::ops::Range;

use specsheet_core::Aabb;

use crate::binary::Reader;
use crate::FormatError;

const VERSION: u8 = 0x15;
const SIGNATURE: &[u8; 3] = b"gmP";
const PART_LEN: usize = 24;
const LOCATOR_LEN: usize = 44;
const PIECE_LEN: usize = 100;
/// Most UV sets a piece may declare. The base archives use at most 4
/// across all 53,917 models; the cap keeps a hostile count from
/// allocating millions of empty sets.
const MAX_UV_SETS: usize = 8;

/// A named group of pieces and locators. Parts are shown or hidden per
/// variant through the matching `.pmd`.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    /// The part's name.
    pub name: String,
    /// Indices into `Pmg::pieces`.
    pub pieces: std::ops::Range<usize>,
    /// Indices into `Pmg::locators`.
    pub locators: std::ops::Range<usize>,
}

/// A named transform in the model, e.g. an accessory attachment point.
#[derive(Clone, Debug, PartialEq)]
pub struct Locator {
    /// The locator's name, e.g. a slot name.
    pub name: String,
    /// Position in model space.
    pub position: [f32; 3],
    /// Uniform scale.
    pub scale: f32,
    /// Rotation quaternion in `(w, x, y, z)` order.
    pub rotation_wxyz: [f32; 4],
    /// The locator's hookup string (e.g. a light or effect id). `None` when
    /// the stored string is empty.
    pub hookup: Option<String>,
}

/// One mesh drawn with one material. Every vector has one entry per vertex,
/// except `indices`.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    /// Index into the material list of the `.pmd` look.
    pub material: u32,
    /// The stored `u32` at piece +8, as read. Hypothesis (models.md,
    /// "Piece"): nibble `n`, from the least significant, is the stored UV
    /// set that feeds UV slot `n`; `0xf` leaves the slot unused. See
    /// [`Piece::uv_set_for_slot`].
    pub uv_routing: u32,
    /// Bounds of the piece's vertices.
    pub aabb: Aabb,
    /// Required: a piece without a position stream is an error.
    pub positions: Vec<[f32; 3]>,
    /// Required: a piece without a normal stream is an error.
    pub normals: Vec<[f32; 3]>,
    /// `(x, y, z, w)` tangents; `None` when the piece has no tangent stream.
    pub tangents: Option<Vec<[f32; 4]>>,
    /// Vertex colours, packed RGBA8 as stored; empty when the piece has no
    /// colour stream.
    pub colors: Vec<u32>,
    /// One `Vec` per UV set; empty when the piece has no UV stream.
    pub uv_sets: Vec<Vec<[f32; 2]>>,
    /// Triangle-list indices into this piece's vertices.
    pub indices: Vec<u16>,
}

impl Piece {
    /// The stored UV set that [`Piece::uv_routing`] names for UV `slot`
    /// (0..8), or `None` when the nibble is `0xf`, the slot is out of range
    /// or the set is not stored (never seen in the game's files).
    pub fn uv_set_for_slot(&self, slot: usize) -> Option<usize> {
        let set = (slot < 8).then(|| ((self.uv_routing >> (4 * slot)) & 0xf) as usize)?;
        (set < self.uv_sets.len()).then_some(set)
    }
}

/// A parsed `.pmg` model geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct Pmg {
    /// Bounds of the whole model.
    pub aabb: Aabb,
    /// The parts, in file order.
    pub parts: Vec<Part>,
    /// Every locator; parts index into it.
    pub locators: Vec<Locator>,
    /// Every piece; parts index into it.
    pub pieces: Vec<Piece>,
    /// Number of skinning bones (0 for rigid models).
    pub bone_count: u32,
}

/// The fixed 0x70-byte header (models.md, "PMG").
struct Header {
    piece_count: usize,
    part_count: usize,
    bone_count: u32,
    locator_count: usize,
    aabb: Aabb,
    parts: usize,
    locators: usize,
    pieces: usize,
    strings: Range<usize>,
    vertices: Range<usize>,
    indices: Range<usize>,
}

impl Header {
    fn read(r: &mut Reader<'_>) -> Result<Self, FormatError> {
        let version = r.u8()?;
        if version != VERSION {
            return Err(r.error(format!("unsupported version {version:#x}")));
        }
        if r.bytes(3)? != SIGNATURE {
            return Err(r.error("bad signature"));
        }
        let piece_count = r.count(PIECE_LEN)?;
        let part_count = r.count(PART_LEN)?;
        let bone_count = r.u32()?;
        let _unknown_10 = r.u32()?;
        let locator_count = r.count(LOCATOR_LEN)?;
        let _unknown_18 = r.u64()?;
        let _bounding_sphere = r.vec4()?;
        let aabb = Aabb {
            min: r.vec3()?,
            max: r.vec3()?,
        };
        let _skeleton = r.u32()?;
        Ok(Self {
            piece_count,
            part_count,
            bone_count,
            locator_count,
            aabb,
            parts: r.u32()? as usize,
            locators: r.u32()? as usize,
            pieces: r.u32()? as usize,
            strings: pool(r)?,
            vertices: pool(r)?,
            indices: pool(r)?,
        })
    }
}

/// Reads an `(offset, size)` pool from the header, checked to lie in the file.
fn pool(r: &mut Reader<'_>) -> Result<Range<usize>, FormatError> {
    let start = r.u32()? as usize;
    let size = r.u32()? as usize;
    start
        .checked_add(size)
        .filter(|&end| end <= r.len())
        .map(|end| start..end)
        .ok_or_else(|| r.error(format!("pool {start:#x}+{size:#x} outside the file")))
}

/// `len` bytes at absolute `offset`, if they lie entirely inside `pool`.
fn slice_in<'a>(
    bytes: &'a [u8],
    pool: &Range<usize>,
    offset: usize,
    len: usize,
) -> Option<&'a [u8]> {
    let end = offset.checked_add(len)?;
    if offset < pool.start || end > pool.end {
        return None;
    }
    bytes.get(offset..end)
}

/// `start..start + count`, checked to lie within `0..total`.
fn index_range(
    r: &Reader<'_>,
    what: &str,
    start: u32,
    count: u32,
    total: usize,
) -> Result<Range<usize>, FormatError> {
    let start = start as usize;
    start
        .checked_add(count as usize)
        .filter(|&end| end <= total)
        .map(|end| start..end)
        .ok_or_else(|| {
            r.error(format!(
                "{what} {start}+{count} outside the {total} present"
            ))
        })
}

/// The vertex stream slots of a piece header, in stored order
/// (models.md, "Piece"). The ninth stored offset is the index stream.
#[derive(Clone, Copy, Debug)]
enum Slot {
    Position,
    Normal,
    Uv,
    Color,
    Color2,
    Tangent,
    BoneIndex,
    BoneWeight,
}

impl Slot {
    const VERTEX: [Slot; 8] = [
        Slot::Position,
        Slot::Normal,
        Slot::Uv,
        Slot::Color,
        Slot::Color2,
        Slot::Tangent,
        Slot::BoneIndex,
        Slot::BoneWeight,
    ];

    /// Bytes per vertex (models.md, "Vertex streams"); UV is per set.
    /// colour2 and the bone streams use the sizes the doc gives the parser:
    /// `u32`, `4 × u8` and `4 × u8`.
    fn size(self) -> usize {
        match self {
            Slot::Position | Slot::Normal => 12,
            Slot::Uv => 8,
            Slot::Tangent => 16,
            Slot::Color | Slot::Color2 | Slot::BoneIndex | Slot::BoneWeight => 4,
        }
    }
}

/// The fixed part of a piece record.
struct PieceHeader {
    index_count: usize,
    vertex_count: usize,
    uv_routing: u32,
    uv_count: usize,
    material: u32,
    aabb: Aabb,
    stride: usize,
    /// Absolute offsets of the vertex streams, in [`Slot::VERTEX`] order;
    /// `None` for an absent stream (stored as a negative offset).
    streams: [Option<usize>; 8],
    index_offset: Option<usize>,
}

impl PieceHeader {
    fn read(r: &mut Reader<'_>) -> Result<Self, FormatError> {
        let index_count = r.u32()? as usize;
        let vertex_count = r.u32()? as usize;
        let uv_routing = r.u32()?;
        let uv_count = r.u32()? as usize;
        let material = r.u32()?;
        let _bounding_sphere = r.vec4()?;
        let aabb = Aabb {
            min: r.vec3()?,
            max: r.vec3()?,
        };
        let stride = r.u32()? as usize;
        let mut offset = || r.i32().map(|o| usize::try_from(o).ok());
        let mut streams = [None; 8];
        for stream in &mut streams {
            *stream = offset()?;
        }
        Ok(Self {
            index_count,
            vertex_count,
            uv_routing,
            uv_count,
            material,
            aabb,
            stride,
            streams,
            index_offset: offset()?,
        })
    }

    fn stream(&self, slot: Slot) -> Option<usize> {
        self.streams[slot as usize]
    }

    /// Bytes per vertex of `slot`, accounting for the number of UV sets.
    fn stream_size(&self, slot: Slot) -> Option<usize> {
        match slot {
            Slot::Uv => slot.size().checked_mul(self.uv_count),
            _ => Some(slot.size()),
        }
    }

    /// Checks the header against itself: required streams, UV count and the
    /// stored stride (which must equal the sum of the present streams, so
    /// the UV sets always fit inside it).
    fn validate(&self, r: &Reader<'_>) -> Result<(), FormatError> {
        for slot in [Slot::Position, Slot::Normal] {
            if self.stream(slot).is_none() {
                return Err(r.error(format!("piece without a {slot:?} stream")));
            }
        }
        if self.uv_count > MAX_UV_SETS {
            return Err(r.error(format!("{} UV sets (at most {MAX_UV_SETS})", self.uv_count)));
        }
        if self.stream(Slot::Uv).is_some() != (self.uv_count > 0) {
            return Err(r.error(format!(
                "{} UV sets disagree with the UV stream's presence",
                self.uv_count
            )));
        }
        let mut expected = 0usize;
        for slot in Slot::VERTEX {
            if self.stream(slot).is_some() {
                expected = self
                    .stream_size(slot)
                    .and_then(|size| expected.checked_add(size))
                    .ok_or_else(|| r.error("vertex stride overflows"))?;
            }
        }
        if self.stride != expected {
            return Err(r.error(format!(
                "vertex stride {} disagrees with the streams present ({expected} bytes)",
                self.stride
            )));
        }
        Ok(())
    }
}

/// Decodes the vertex and index streams of one piece. Streams are
/// interleaved: element `i` of a stream sits at its offset + `i × stride`.
struct PieceData<'a> {
    r: &'a Reader<'a>,
    bytes: &'a [u8],
    header: &'a Header,
    piece: &'a PieceHeader,
}

impl PieceData<'_> {
    /// The bytes from a stream's first element to the end of its last, or
    /// `None` if the piece lacks the stream.
    fn stream_bytes(&self, slot: Slot) -> Result<Option<&[u8]>, FormatError> {
        let Some(offset) = self.piece.stream(slot) else {
            return Ok(None);
        };
        let Some(last) = self.piece.vertex_count.checked_sub(1) else {
            return Ok(Some(&[]));
        };
        last.checked_mul(self.piece.stride)
            .and_then(|span| span.checked_add(self.piece.stream_size(slot)?))
            .and_then(|len| slice_in(self.bytes, &self.header.vertices, offset, len))
            .map(Some)
            .ok_or_else(|| {
                self.r.error(format!(
                    "{slot:?} stream at {offset:#x} for {} vertices outside the vertex pool",
                    self.piece.vertex_count
                ))
            })
    }

    /// `N` little-endian 32-bit words per vertex, `skip` bytes into each
    /// element of `slot`.
    fn words<const N: usize>(
        &self,
        slot: Slot,
        skip: usize,
    ) -> Result<Option<Vec<[u32; N]>>, FormatError> {
        let stride = self.piece.stride;
        Ok(self.stream_bytes(slot)?.map(|bytes| {
            bytes
                .chunks(stride.max(1))
                .map(|element| std::array::from_fn(|i| word_at(element, skip + 4 * i)))
                .collect()
        }))
    }

    fn floats<const N: usize>(
        &self,
        slot: Slot,
        skip: usize,
    ) -> Result<Option<Vec<[f32; N]>>, FormatError> {
        Ok(self
            .words::<N>(slot, skip)?
            .map(|words| words.into_iter().map(|w| w.map(f32::from_bits)).collect()))
    }

    fn required<const N: usize>(&self, slot: Slot) -> Result<Vec<[f32; N]>, FormatError> {
        self.floats(slot, 0)?
            .ok_or_else(|| self.r.error(format!("piece without a {slot:?} stream")))
    }

    /// The `u16` triangle list; every index must name a vertex of the piece.
    fn indices(&self) -> Result<Vec<u16>, FormatError> {
        let count = self.piece.index_count;
        if count == 0 {
            return Ok(Vec::new());
        }
        let bytes = self
            .piece
            .index_offset
            .zip(count.checked_mul(2))
            .and_then(|(offset, len)| slice_in(self.bytes, &self.header.indices, offset, len))
            .ok_or_else(|| {
                self.r
                    .error(format!("{count} indices outside the index pool"))
            })?;
        let mut reader = Reader::new("pmg indices", bytes);
        let indices = (0..count)
            .map(|_| reader.u16())
            .collect::<Result<Vec<_>, _>>()?;
        match indices
            .iter()
            .find(|&&i| usize::from(i) >= self.piece.vertex_count)
        {
            Some(bad) => Err(self.r.error(format!(
                "index {bad} out of range for {} vertices",
                self.piece.vertex_count
            ))),
            None => Ok(indices),
        }
    }

    fn decode(&self) -> Result<Piece, FormatError> {
        // The UV sets sit side by side inside each UV element.
        let uv_sets = (0..self.piece.uv_count)
            .map(|set| Ok(self.floats(Slot::Uv, 8 * set)?.unwrap_or_default()))
            .collect::<Result<_, FormatError>>()?;
        Ok(Piece {
            material: self.piece.material,
            uv_routing: self.piece.uv_routing,
            aabb: self.piece.aabb,
            positions: self.required(Slot::Position)?,
            normals: self.required(Slot::Normal)?,
            tangents: self.floats(Slot::Tangent, 0)?,
            colors: self
                .words::<1>(Slot::Color, 0)?
                .map(|words| words.into_iter().map(|[w]| w).collect())
                .unwrap_or_default(),
            uv_sets,
            indices: self.indices()?,
        })
    }
}

/// Little-endian `u32` at `at` (0 if `bytes` is too short; stream sizes are
/// checked before decoding, so this never happens for valid offsets).
fn word_at(bytes: &[u8], at: usize) -> u32 {
    let mut word = [0; 4];
    if let Some(src) = at.checked_add(4).and_then(|end| bytes.get(at..end)) {
        word.copy_from_slice(src);
    }
    u32::from_le_bytes(word)
}

impl Pmg {
    /// Parses a `.pmg` file. Malformed input (wrong signature or version,
    /// out-of-range offsets or counts, a stored vertex stride that disagrees
    /// with the streams present) returns an error and never panics.
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        let mut r = Reader::new("pmg", bytes);
        let header = Header::read(&mut r)?;
        let strings = Reader::new(
            "pmg string pool",
            bytes.get(header.strings.clone()).unwrap_or_default(),
        );

        r.seek(header.parts)?;
        let mut parts = Vec::with_capacity(header.part_count);
        for _ in 0..header.part_count {
            let name = r.token()?.decode();
            let (piece_count, piece_start) = (r.u32()?, r.u32()?);
            let (locator_count, locator_start) = (r.u32()?, r.u32()?);
            parts.push(Part {
                name,
                pieces: index_range(&r, "pieces", piece_start, piece_count, header.piece_count)?,
                locators: index_range(
                    &r,
                    "locators",
                    locator_start,
                    locator_count,
                    header.locator_count,
                )?,
            });
        }

        // Hookup strings are shared between locators; capping the decoded
        // text at the file size keeps a hostile file from multiplying memory.
        let mut budget = bytes.len();
        r.seek(header.locators)?;
        let mut locators = Vec::with_capacity(header.locator_count);
        for _ in 0..header.locator_count {
            let name = r.token()?.decode();
            let position = r.vec3()?;
            let scale = r.f32()?;
            let rotation_wxyz = r.vec4()?;
            let hookup = match usize::try_from(r.i32()?) {
                Ok(offset) => Some(strings.c_string_at(offset)?).filter(|s| !s.is_empty()),
                Err(_) => None,
            };
            budget = budget
                .checked_sub(hookup.as_ref().map_or(0, String::len))
                .ok_or_else(|| r.error("hookup strings exceed the file size"))?;
            locators.push(Locator {
                name,
                position,
                scale,
                rotation_wxyz,
                hookup,
            });
        }

        // In game data the pieces' streams never overlap, so together they
        // decode at most the pool sizes. Holding every file to that stops
        // pieces that share one range from multiplying memory.
        let mut vertex_budget = header.vertices.len();
        let mut index_budget = header.indices.len();
        r.seek(header.pieces)?;
        let mut pieces = Vec::with_capacity(header.piece_count);
        for _ in 0..header.piece_count {
            let piece = PieceHeader::read(&mut r)?;
            piece.validate(&r)?;
            let spend = |budget: usize, count: usize, size: usize, pool: &str| {
                count
                    .checked_mul(size)
                    .and_then(|bytes| budget.checked_sub(bytes))
                    .ok_or_else(|| r.error(format!("pieces decode more than the {pool} pool")))
            };
            vertex_budget = spend(vertex_budget, piece.vertex_count, piece.stride, "vertex")?;
            index_budget = spend(index_budget, piece.index_count, 2, "index")?;
            let data = PieceData {
                r: &r,
                bytes,
                header: &header,
                piece: &piece,
            };
            pieces.push(data.decode()?);
        }

        Ok(Self {
            aabb: header.aabb,
            parts,
            locators,
            pieces,
            bone_count: header.bone_count,
        })
    }

    /// The part with exactly this name (case-sensitive).
    pub fn part(&self, name: &str) -> Option<&Part> {
        self.parts.iter().find(|part| part.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::Token;

    /// One piece to lay out: which optional streams it has.
    #[derive(Clone)]
    struct PieceSpec {
        vertices: usize,
        uv_sets: usize,
        tangents: bool,
        color: bool,
        bones: bool,
        /// The `u32` stored at piece +8.
        uv_routing: u32,
        indices: Vec<u16>,
    }

    impl Default for PieceSpec {
        fn default() -> Self {
            Self {
                vertices: 3,
                uv_sets: 1,
                tangents: false,
                color: true,
                bones: false,
                uv_routing: 0xffff_fff0,
                indices: vec![0, 1, 2],
            }
        }
    }

    struct Fixture {
        /// name, piece count, piece start, locator count, locator start.
        parts: Vec<(&'static str, u32, u32, u32, u32)>,
        /// name, hookup.
        locators: Vec<(&'static str, &'static str)>,
        pieces: Vec<PieceSpec>,
    }

    fn fixture() -> Fixture {
        Fixture {
            parts: vec![("defaultpart", 2, 0, 2, 0), ("uk_part", 0, 2, 0, 2)],
            locators: vec![("r_light", ""), ("ml1", "flare.vehicle.beacon")],
            pieces: vec![
                PieceSpec {
                    uv_sets: 2,
                    tangents: true,
                    uv_routing: 0xf1ff_fff0,
                    ..PieceSpec::default()
                },
                PieceSpec {
                    vertices: 4,
                    uv_sets: 0,
                    color: false,
                    indices: vec![0, 1, 2, 2, 3, 0],
                    ..PieceSpec::default()
                },
            ],
        }
    }

    fn f32s(out: &mut Vec<u8>, values: &[f32]) {
        for value in values {
            out.extend(value.to_le_bytes());
        }
    }

    fn u32s(out: &mut Vec<u8>, values: &[u32]) {
        for value in values {
            out.extend(value.to_le_bytes());
        }
    }

    fn put(out: &mut [u8], at: usize, value: u32) {
        out[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn get(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    fn position(vertex: usize) -> [f32; 3] {
        [vertex as f32, 0.5, -(vertex as f32)]
    }

    /// Lays a file out as models.md, "PMG" describes: header, parts,
    /// locators, piece headers, then the string, vertex and index pools.
    fn build(f: &Fixture) -> Vec<u8> {
        let parts_at = 0x70;
        let locators_at = parts_at + PART_LEN * f.parts.len();
        let pieces_at = locators_at + LOCATOR_LEN * f.locators.len();
        let strings_at = pieces_at + PIECE_LEN * f.pieces.len();

        // String pool: an empty string first, as in the game's files.
        let mut strings = vec![0u8];
        let hookups: Vec<u32> = f
            .locators
            .iter()
            .map(|(_, hookup)| {
                if hookup.is_empty() {
                    return 0;
                }
                let at = strings.len() as u32;
                strings.extend(hookup.bytes());
                strings.push(0);
                at
            })
            .collect();
        let vertices_at = strings_at + strings.len();

        // Vertex pool: interleaved position, normal, [tangent], [colour],
        // [bone index, bone weight], [uv × n].
        let mut vertices = Vec::new();
        let mut stream_offsets = Vec::new();
        for piece in &f.pieces {
            let base = vertices_at + vertices.len();
            let mut layout = [-1i32; 8];
            let mut at = 0usize;
            let mut slot = |slot: Slot, size: usize, present: bool| {
                if present {
                    layout[slot as usize] = (base + at) as i32;
                    at += size;
                }
            };
            slot(Slot::Position, 12, true);
            slot(Slot::Normal, 12, true);
            slot(Slot::Tangent, 16, piece.tangents);
            slot(Slot::Color, 4, piece.color);
            slot(Slot::BoneIndex, 4, piece.bones);
            slot(Slot::BoneWeight, 4, piece.bones);
            slot(Slot::Uv, 8 * piece.uv_sets, piece.uv_sets > 0);
            for v in 0..piece.vertices {
                f32s(&mut vertices, &position(v));
                f32s(&mut vertices, &[0.0, 1.0, 0.0]);
                if piece.tangents {
                    f32s(&mut vertices, &[1.0, 0.0, 0.0, -1.0]);
                }
                if piece.color {
                    u32s(&mut vertices, &[0xff00_0000 | v as u32]);
                }
                if piece.bones {
                    vertices.extend([0, 1, 2, 3, 255, 0, 0, 0]);
                }
                for set in 0..piece.uv_sets {
                    f32s(&mut vertices, &[v as f32 / 4.0, set as f32]);
                }
            }
            stream_offsets.push((layout, at));
        }
        let indices_at = vertices_at + vertices.len();

        // Index pool: each list starts 4-byte aligned.
        let mut indices = Vec::new();
        let mut index_offsets = Vec::new();
        for piece in &f.pieces {
            index_offsets.push(indices_at + indices.len());
            for index in &piece.indices {
                indices.extend(index.to_le_bytes());
            }
            indices.resize(indices.len().next_multiple_of(4), 0);
        }

        let mut out = vec![0x15, b'g', b'm', b'P'];
        let counts = [f.pieces.len(), f.parts.len(), 0, 0, f.locators.len()];
        u32s(&mut out, &counts.map(|c| c as u32));
        out.extend(0u64.to_le_bytes());
        f32s(&mut out, &[0.0, 0.5, 0.0, 10.0]);
        f32s(&mut out, &[-10.0, -10.0, -10.0, 10.0, 10.0, 10.0]);
        let offsets = [
            0x70,
            parts_at,
            locators_at,
            pieces_at,
            strings_at,
            strings.len(),
            vertices_at,
            vertices.len(),
            indices_at,
            indices.len(),
        ];
        u32s(&mut out, &offsets.map(|o| o as u32));
        assert_eq!(out.len(), 0x70);

        for &(name, piece_count, piece_start, locator_count, locator_start) in &f.parts {
            out.extend(Token::encode(name).unwrap().0.to_le_bytes());
            u32s(
                &mut out,
                &[piece_count, piece_start, locator_count, locator_start],
            );
        }
        for (i, (name, _)) in f.locators.iter().enumerate() {
            out.extend(Token::encode(name).unwrap().0.to_le_bytes());
            f32s(&mut out, &[1.0, 2.0, 3.0, 1.0]);
            f32s(&mut out, &[0.0, 0.0, 1.0, 0.0]);
            out.extend((hookups[i] as i32).to_le_bytes());
        }
        for (i, piece) in f.pieces.iter().enumerate() {
            let (layout, stride) = stream_offsets[i];
            let counts = [piece.indices.len(), piece.vertices];
            u32s(&mut out, &counts.map(|c| c as u32));
            u32s(
                &mut out,
                &[piece.uv_routing, piece.uv_sets as u32, i as u32],
            );
            f32s(&mut out, &[0.0, 0.0, 0.0, 5.0]);
            f32s(&mut out, &[-1.0, -1.0, -1.0, 4.0, 1.0, 1.0]);
            u32s(&mut out, &[stride as u32]);
            for offset in layout {
                out.extend(offset.to_le_bytes());
            }
            out.extend((index_offsets[i] as i32).to_le_bytes());
        }
        out.extend(strings);
        out.extend(vertices);
        out.extend(indices);
        out
    }

    /// Absolute offset of field `field` of piece `piece`.
    fn piece_field(bytes: &[u8], piece: usize, field: usize) -> usize {
        get(bytes, 0x54) as usize + PIECE_LEN * piece + field
    }

    fn patched(bytes: &[u8], at: usize, value: u32) -> Result<Pmg, FormatError> {
        let mut bytes = bytes.to_vec();
        put(&mut bytes, at, value);
        Pmg::parse(&bytes)
    }

    #[test]
    fn parses_parts_locators_and_pieces() {
        let pmg = Pmg::parse(&build(&fixture())).unwrap();
        assert_eq!(pmg.aabb.max, [10.0; 3]);
        assert_eq!(pmg.bone_count, 0);
        assert_eq!(
            pmg.parts[0],
            Part {
                name: "defaultpart".into(),
                pieces: 0..2,
                locators: 0..2,
            }
        );
        assert_eq!(pmg.parts[1].pieces, 2..2);

        let light = &pmg.locators[0];
        assert_eq!(light.name, "r_light");
        assert_eq!(light.position, [1.0, 2.0, 3.0]);
        assert_eq!(light.scale, 1.0);
        assert_eq!(light.rotation_wxyz, [0.0, 0.0, 1.0, 0.0]);

        let piece = &pmg.pieces[0];
        assert_eq!(piece.material, 0);
        assert_eq!(piece.uv_routing, 0xf1ff_fff0);
        assert_eq!(piece.uv_set_for_slot(0), Some(0));
        assert_eq!(piece.uv_set_for_slot(6), Some(1));
        assert_eq!(piece.uv_set_for_slot(1), None);
        assert_eq!(piece.uv_set_for_slot(8), None);
        assert_eq!(piece.aabb.max, [4.0, 1.0, 1.0]);
        assert_eq!(piece.positions, (0..3).map(position).collect::<Vec<_>>());
        assert_eq!(piece.normals, vec![[0.0, 1.0, 0.0]; 3]);
        assert_eq!(piece.tangents, Some(vec![[1.0, 0.0, 0.0, -1.0]; 3]));
        assert_eq!(piece.colors, [0xff00_0000, 0xff00_0001, 0xff00_0002]);
        assert_eq!(piece.uv_sets.len(), 2);
        assert_eq!(piece.uv_sets[0], [[0.0, 0.0], [0.25, 0.0], [0.5, 0.0]]);
        assert_eq!(piece.uv_sets[1], [[0.0, 1.0], [0.25, 1.0], [0.5, 1.0]]);
        assert_eq!(piece.indices, [0, 1, 2]);
    }

    #[test]
    fn optional_streams_may_be_absent() {
        let pmg = Pmg::parse(&build(&fixture())).unwrap();
        let piece = &pmg.pieces[1];
        assert_eq!(piece.material, 1);
        assert_eq!(piece.uv_routing, 0xffff_fff0);
        // Slot 0 names set 0, but this piece stores no UV sets.
        assert_eq!(piece.uv_set_for_slot(0), None);
        assert_eq!(piece.positions.len(), 4);
        assert_eq!(piece.tangents, None);
        assert!(piece.colors.is_empty());
        assert!(piece.uv_sets.is_empty());
        // The odd-length index list of piece 0 was padded to 4 bytes.
        assert_eq!(piece.indices, [0, 1, 2, 2, 3, 0]);
    }

    #[test]
    fn empty_hookups_are_none() {
        let pmg = Pmg::parse(&build(&fixture())).unwrap();
        assert_eq!(pmg.locators[0].hookup, None);
        assert_eq!(
            pmg.locators[1].hookup.as_deref(),
            Some("flare.vehicle.beacon")
        );
    }

    #[test]
    fn accepts_skinned_pieces() {
        let mut f = fixture();
        f.pieces[0].bones = true;
        let mut bytes = build(&f);
        put(&mut bytes, 0x0c, 12);
        let pmg = Pmg::parse(&bytes).unwrap();
        assert_eq!(pmg.bone_count, 12);
        assert_eq!(
            pmg.pieces[0].positions,
            (0..3).map(position).collect::<Vec<_>>()
        );
        assert_eq!(pmg.pieces[0].uv_sets[1][2], [0.5, 1.0]);
    }

    #[test]
    fn finds_parts_by_exact_name() {
        let pmg = Pmg::parse(&build(&fixture())).unwrap();
        assert_eq!(pmg.part("uk_part").map(|p| p.pieces.clone()), Some(2..2));
        assert_eq!(pmg.part("UK_PART"), None);
    }

    #[test]
    fn rejects_bad_signature_and_version() {
        let good = build(&fixture());
        for (at, byte) in [(0, 0x14), (1, b'x'), (3, b'p')] {
            let mut bytes = good.clone();
            bytes[at] = byte;
            assert!(Pmg::parse(&bytes).is_err(), "byte {at}");
        }
    }

    #[test]
    fn rejects_every_truncation() {
        let bytes = build(&fixture());
        for len in 0..bytes.len() {
            assert!(Pmg::parse(&bytes[..len]).is_err(), "length {len}");
        }
    }

    #[test]
    fn rejects_bad_counts_and_offsets() {
        let good = build(&fixture());
        for at in [0x04, 0x08, 0x14] {
            assert!(patched(&good, at, u32::MAX).is_err(), "count {at:#x}");
        }
        for at in (0x4c..0x70).step_by(4) {
            assert!(patched(&good, at, u32::MAX).is_err(), "offset {at:#x}");
        }
    }

    #[test]
    fn rejects_part_ranges_past_the_pieces_or_locators() {
        let good = build(&fixture());
        let part1 = 0x70 + PART_LEN;
        assert!(patched(&good, part1 + 8, 1).is_err(), "piece count");
        assert!(patched(&good, part1 + 12, 3).is_err(), "piece start");
        assert!(patched(&good, part1 + 16, 1).is_err(), "locator count");
        assert!(patched(&good, 0x70 + 12, u32::MAX).is_err(), "overflow");
    }

    #[test]
    fn rejects_a_stride_that_disagrees_with_the_streams() {
        let good = build(&fixture());
        let stride = piece_field(&good, 0, 60);
        // 12 + 12 + 16 + 4 + 2 × 8 bytes.
        assert_eq!(get(&good, stride), 60);
        assert!(patched(&good, stride, 56).is_err());
        assert!(patched(&good, stride, 64).is_err());
        // Dropping the tangent stream without fixing the stride.
        assert!(patched(&good, piece_field(&good, 0, 84), u32::MAX).is_err());
    }

    #[test]
    fn requires_positions_and_normals() {
        let good = build(&fixture());
        // Piece 1 is position + normal only (stride 24).
        for field in [64, 68] {
            let mut bytes = good.clone();
            put(&mut bytes, piece_field(&good, 1, field), u32::MAX);
            put(&mut bytes, piece_field(&good, 1, 60), 12);
            let error = Pmg::parse(&bytes).unwrap_err();
            assert!(error.message.contains("without"), "{error}");
        }
    }

    #[test]
    fn rejects_uv_counts_that_disagree_with_the_uv_stream() {
        let good = build(&fixture());
        assert!(patched(&good, piece_field(&good, 0, 12), 0).is_err());
        assert!(patched(&good, piece_field(&good, 1, 12), 1).is_err());
        assert!(patched(&good, piece_field(&good, 0, 12), u32::MAX).is_err());
    }

    #[test]
    fn rejects_streams_outside_their_pools() {
        let good = build(&fixture());
        let strings = get(&good, 0x58);
        let vertices_end = get(&good, 0x60) + get(&good, 0x64);
        let position = piece_field(&good, 1, 64);
        assert!(patched(&good, position, strings).is_err());
        assert!(patched(&good, position, vertices_end - 12).is_err());
        assert!(patched(&good, piece_field(&good, 1, 4), u32::MAX).is_err());
        let index = piece_field(&good, 1, 96);
        assert!(patched(&good, index, vertices_end - 12).is_err());
        assert!(patched(&good, index, u32::MAX).is_err());
        assert!(patched(&good, piece_field(&good, 1, 0), 1000).is_err());
    }

    #[test]
    fn rejects_indices_past_the_vertices() {
        let mut f = fixture();
        f.pieces[1].indices[4] = 4;
        let error = Pmg::parse(&build(&f)).unwrap_err();
        assert!(error.message.contains("index 4"), "{error}");
    }

    #[test]
    fn rejects_bad_hookup_offsets() {
        let good = build(&fixture());
        let hookup = 0x70 + 2 * PART_LEN + 40;
        let pool = get(&good, 0x5c);
        assert!(patched(&good, hookup, pool).is_err());
        // A negative offset is treated as "no hookup".
        let pmg = patched(&good, hookup, u32::MAX).unwrap();
        assert_eq!(pmg.locators[0].hookup, None);
    }

    #[test]
    fn rejects_a_huge_uv_set_count() {
        // A piece without vertices that claims a million UV sets, with a
        // stride to match, must not allocate a million empty sets.
        let mut f = fixture();
        f.pieces[1] = PieceSpec {
            vertices: 0,
            uv_sets: 1,
            indices: vec![],
            ..f.pieces[1].clone()
        };
        let good = build(&f);
        assert!(Pmg::parse(&good).is_ok());
        let mut bytes = good.clone();
        put(&mut bytes, piece_field(&good, 1, 12), 1_000_000);
        put(&mut bytes, piece_field(&good, 1, 60), 24 + 8 * 1_000_000);
        assert!(Pmg::parse(&bytes).is_err());
    }

    #[test]
    fn rejects_pieces_sharing_one_stream_range() {
        // Pieces 1 and 2 are re-pointed at piece 0's streams: decoding them
        // would read more bytes than the pools hold.
        let mut f = fixture();
        f.pieces[0].vertices = 40;
        let small = PieceSpec {
            vertices: 1,
            indices: vec![0, 0, 0],
            ..PieceSpec::default()
        };
        f.pieces = vec![f.pieces[0].clone(), small.clone(), small];
        f.parts = vec![("defaultpart", 3, 0, 2, 0)];
        let good = build(&f);
        assert!(Pmg::parse(&good).is_ok());
        let mut bytes = good.clone();
        let first = piece_field(&good, 0, 0);
        let header = good[first..first + PIECE_LEN].to_vec();
        for piece in 1..3 {
            let at = piece_field(&good, piece, 0);
            bytes[at..at + PIECE_LEN].copy_from_slice(&header);
        }
        let error = Pmg::parse(&bytes).unwrap_err();
        assert!(error.message.contains("pool"), "{error}");
    }

    #[test]
    fn aabb_helpers() {
        let a = Aabb {
            min: [0.0, -2.0, 1.0],
            max: [4.0, 2.0, 2.0],
        };
        assert_eq!(a.center(), [2.0, 0.0, 1.5]);
        let b = Aabb {
            min: [-1.0, 0.0, 0.0],
            max: [1.0, 3.0, 1.0],
        };
        assert_eq!(
            a.union(&b),
            Aabb {
                min: [-1.0, -2.0, 0.0],
                max: [4.0, 3.0, 2.0],
            }
        );
        // z is only 1 wide, so a 1.0 margin collapses it to its centre.
        assert_eq!(
            a.shrunk(1.0),
            Aabb {
                min: [1.0, -1.0, 1.5],
                max: [3.0, 1.0, 1.5],
            }
        );
        assert!(a.contains_point([4.0, -2.0, 1.0]));
        assert!(!a.contains_point([4.1, 0.0, 1.5]));
        assert!(a.contains_box(&a));
        assert!(a.contains_box(&a.shrunk(0.25)));
        assert!(!a.contains_box(&b));
    }
}
