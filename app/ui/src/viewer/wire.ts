// Decoders for the backend's binary frames (see app/src-tauri/src/wire.rs).

export type BlendMode = "opaque" | "alphaTest" | "blend" | "additive" | "hidden";

export interface MaterialHeader {
  effect: string;
  blend: BlendMode;
  diffuse: [number, number, number];
  baseTexture: string | null;
  paintable: boolean;
  emissive: boolean;
  decal: boolean;
}

export interface PieceData {
  material: number;
  positions: Float32Array;
  normals: Float32Array;
  uvs: Float32Array | null;
  indices: Uint32Array;
}

export interface ModelData {
  materials: MaterialHeader[];
  pieces: PieceData[];
  aabbMin: [number, number, number];
  aabbMax: [number, number, number];
}

export type TextureFormat = "bc1" | "bc2" | "bc3" | "bc4" | "bc5" | "rgba8" | "r8";

export interface TextureData {
  format: TextureFormat;
  srgb: boolean;
  repeatU: boolean;
  repeatV: boolean;
  mips: { width: number; height: number; data: Uint8Array }[];
}

interface Frame {
  header: unknown;
  binary: ArrayBuffer;
}

function readFrame(buffer: ArrayBuffer, magic: string): Frame {
  const view = new DataView(buffer);
  const found = String.fromCharCode(...new Uint8Array(buffer, 0, 4));
  if (found !== magic) throw new Error(`expected ${magic} frame, got ${found}`);
  const version = view.getUint32(4, true);
  if (version !== 1) throw new Error(`unsupported frame version ${version}`);
  const jsonLength = view.getUint32(8, true);
  const json = new TextDecoder().decode(new Uint8Array(buffer, 12, jsonLength));
  return { header: JSON.parse(json), binary: buffer.slice(12 + jsonLength) };
}

interface ModelHeader {
  materials: MaterialHeader[];
  pieces: {
    material: number;
    vertexCount: number;
    indexCount: number;
    positions: number;
    normals: number;
    uvs: number | null;
    indices: number;
  }[];
  aabbMin: [number, number, number];
  aabbMax: [number, number, number];
}

export function decodeModel(buffer: ArrayBuffer): ModelData {
  const { header, binary } = readFrame(buffer, "ETSM");
  const h = header as ModelHeader;
  return {
    materials: h.materials,
    aabbMin: h.aabbMin,
    aabbMax: h.aabbMax,
    pieces: h.pieces.map((p) => ({
      material: p.material,
      positions: new Float32Array(binary, p.positions, p.vertexCount * 3),
      normals: new Float32Array(binary, p.normals, p.vertexCount * 3),
      uvs: p.uvs === null ? null : new Float32Array(binary, p.uvs, p.vertexCount * 2),
      indices: new Uint32Array(binary, p.indices, p.indexCount),
    })),
  };
}

interface TextureHeader {
  format: TextureFormat;
  srgb: boolean;
  repeatU: boolean;
  repeatV: boolean;
  mips: { width: number; height: number; offset: number; length: number }[];
}

export function decodeTexture(buffer: ArrayBuffer): TextureData {
  const { header, binary } = readFrame(buffer, "ETST");
  const h = header as TextureHeader;
  return {
    format: h.format,
    srgb: h.srgb,
    repeatU: h.repeatU,
    repeatV: h.repeatV,
    mips: h.mips.map((m) => ({ width: m.width, height: m.height, data: new Uint8Array(binary, m.offset, m.length) })),
  };
}
