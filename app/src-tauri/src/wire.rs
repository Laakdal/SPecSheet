//! Binary payloads served over `ets2://` (decoded by
//! `app/ui/src/viewer/wire.ts`):
//!
//! ```text
//! magic [4]u8 ("ETSM" model / "ETST" texture) | u32 version | u32 json_len
//! json (UTF-8, padded with spaces to a multiple of 4)
//! binary section; every offset in the JSON is relative to its start and
//! 4-byte aligned
//! ```

use game_data::render::{BlendMode, MeshData, TextureData, TextureFormat};
use serde::Serialize;

const VERSION: u32 = 1;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelHeader<'a> {
    materials: Vec<MaterialHeader<'a>>,
    pieces: Vec<PieceHeader>,
    aabb_min: [f32; 3],
    aabb_max: [f32; 3],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MaterialHeader<'a> {
    effect: &'a str,
    blend: &'static str,
    diffuse: [f32; 3],
    base_texture: Option<&'a str>,
    paintable: bool,
    emissive: bool,
    decal: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PieceHeader {
    material: usize,
    vertex_count: usize,
    index_count: usize,
    positions: usize,
    normals: usize,
    uvs: Option<usize>,
    indices: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TextureHeader {
    format: &'static str,
    srgb: bool,
    repeat_u: bool,
    repeat_v: bool,
    mips: Vec<MipHeader>,
}

#[derive(Serialize)]
struct MipHeader {
    width: u32,
    height: u32,
    offset: usize,
    length: usize,
}

pub fn encode_model(mesh: &MeshData) -> Result<Vec<u8>, serde_json::Error> {
    let mut binary = Vec::new();
    let mut pieces = Vec::with_capacity(mesh.pieces.len());
    for piece in &mesh.pieces {
        let positions = push_f32s(&mut binary, piece.positions.iter().flatten());
        let normals = push_f32s(&mut binary, piece.normals.iter().flatten());
        let uvs = piece
            .uvs
            .as_ref()
            .map(|uvs| push_f32s(&mut binary, uvs.iter().flatten()));
        let indices = binary.len();
        for index in &piece.indices {
            binary.extend_from_slice(&index.to_le_bytes());
        }
        pieces.push(PieceHeader {
            material: piece.material,
            vertex_count: piece.positions.len(),
            index_count: piece.indices.len(),
            positions,
            normals,
            uvs,
            indices,
        });
    }
    let header = ModelHeader {
        materials: mesh
            .materials
            .iter()
            .map(|m| MaterialHeader {
                effect: &m.effect,
                blend: match m.blend {
                    BlendMode::Opaque => "opaque",
                    BlendMode::AlphaTest => "alphaTest",
                    BlendMode::Blend => "blend",
                    BlendMode::Additive => "additive",
                    BlendMode::Hidden => "hidden",
                },
                diffuse: m.diffuse,
                base_texture: m.base_texture.as_deref(),
                paintable: m.paintable,
                emissive: m.emissive,
                decal: m.decal,
            })
            .collect(),
        pieces,
        aabb_min: mesh.aabb_min,
        aabb_max: mesh.aabb_max,
    };
    frame(b"ETSM", &serde_json::to_vec(&header)?, binary)
}

pub fn encode_texture(texture: &TextureData) -> Result<Vec<u8>, serde_json::Error> {
    let mut binary = Vec::new();
    let mut mips = Vec::with_capacity(texture.mips.len());
    for mip in &texture.mips {
        let offset = binary.len();
        binary.extend_from_slice(&mip.data);
        pad4(&mut binary, 0);
        mips.push(MipHeader {
            width: mip.width,
            height: mip.height,
            offset,
            length: mip.data.len(),
        });
    }
    let header = TextureHeader {
        format: match texture.format {
            TextureFormat::Bc1 => "bc1",
            TextureFormat::Bc2 => "bc2",
            TextureFormat::Bc3 => "bc3",
            TextureFormat::Bc4 => "bc4",
            TextureFormat::Bc5 => "bc5",
            TextureFormat::Rgba8 => "rgba8",
            TextureFormat::R8 => "r8",
        },
        srgb: texture.srgb,
        repeat_u: texture.repeat_u,
        repeat_v: texture.repeat_v,
        mips,
    };
    frame(b"ETST", &serde_json::to_vec(&header)?, binary)
}

fn frame(magic: &[u8; 4], json: &[u8], binary: Vec<u8>) -> Result<Vec<u8>, serde_json::Error> {
    let mut out = Vec::with_capacity(12 + json.len() + 3 + binary.len());
    out.extend_from_slice(magic);
    out.extend_from_slice(&VERSION.to_le_bytes());
    let padded = json.len().next_multiple_of(4);
    out.extend_from_slice(&(padded as u32).to_le_bytes());
    out.extend_from_slice(json);
    out.resize(12 + padded, b' ');
    out.extend_from_slice(&binary);
    Ok(out)
}

fn push_f32s<'a>(binary: &mut Vec<u8>, values: impl Iterator<Item = &'a f32>) -> usize {
    let offset = binary.len();
    for value in values {
        binary.extend_from_slice(&value.to_le_bytes());
    }
    offset
}

fn pad4(binary: &mut Vec<u8>, fill: u8) {
    let len = binary.len().next_multiple_of(4);
    binary.resize(len, fill);
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_data::render::{MaterialInfo, MeshPiece, MipLevel};

    fn read_header(bytes: &[u8]) -> (String, serde_json::Value, &[u8]) {
        let magic = String::from_utf8_lossy(&bytes[..4]).into_owned();
        let len = u32::from_le_bytes(bytes[8..12].try_into().expect("len")) as usize;
        let json = serde_json::from_slice(&bytes[12..12 + len]).expect("json");
        (magic, json, &bytes[12 + len..])
    }

    #[test]
    fn model_frame_offsets_point_at_data() {
        let mesh = MeshData {
            path: "/m.pmd".into(),
            materials: vec![MaterialInfo {
                path: "/m.mat".into(),
                effect: "eut2.dif".into(),
                blend: BlendMode::Opaque,
                diffuse: [1.0; 3],
                base_texture: None,
                paintable: false,
                emissive: false,
                decal: false,
            }],
            pieces: vec![MeshPiece {
                material: 0,
                positions: vec![[1.0, 2.0, 3.0]; 3],
                normals: vec![[0.0, 1.0, 0.0]; 3],
                uvs: None,
                indices: vec![0, 2, 1],
            }],
            aabb_min: [0.0; 3],
            aabb_max: [1.0; 3],
        };
        let bytes = encode_model(&mesh).expect("encode");
        let (magic, json, binary) = read_header(&bytes);
        assert_eq!(magic, "ETSM");
        assert_eq!(
            (bytes.len() - binary.len()) % 4,
            0,
            "binary section aligned"
        );
        let piece = &json["pieces"][0];
        let at = piece["indices"].as_u64().expect("offset") as usize;
        assert_eq!(&binary[at + 4..at + 8], &2u32.to_le_bytes());
        let at = piece["positions"].as_u64().expect("offset") as usize;
        assert_eq!(&binary[at + 4..at + 8], &2.0f32.to_le_bytes());
    }

    #[test]
    fn texture_mips_are_aligned() {
        let texture = TextureData {
            format: TextureFormat::Bc1,
            srgb: true,
            mips: vec![
                MipLevel {
                    width: 4,
                    height: 4,
                    data: vec![1; 8],
                },
                MipLevel {
                    width: 2,
                    height: 2,
                    data: vec![2; 3],
                },
                MipLevel {
                    width: 1,
                    height: 1,
                    data: vec![3; 8],
                },
            ],
            repeat_u: true,
            repeat_v: false,
        };
        let bytes = encode_texture(&texture).expect("encode");
        let (_, json, binary) = read_header(&bytes);
        let last = &json["mips"][2];
        let at = last["offset"].as_u64().expect("offset") as usize;
        assert_eq!(at % 4, 0);
        assert_eq!(binary[at], 3);
    }
}
