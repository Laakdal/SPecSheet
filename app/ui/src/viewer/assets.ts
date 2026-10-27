// Fetches converted game assets and turns them into three.js resources.
// Every model (path + look + variant) and texture is fetched once and shared.

import * as THREE from "three";
import { assetUrl } from "../api";
import { decodeModel, decodeTexture, type MaterialHeader, type ModelData, type TextureData } from "./wire";

export interface ModelAsset {
  /** Template meshes; instances clone them (sharing geometry and materials). */
  meshes: THREE.Mesh[];
  /** Materials painted with the truck's paint-job colour. */
  paintable: THREE.MeshStandardMaterial[];
}

async function fetchBuffer(url: string): Promise<ArrayBuffer> {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${response.status} ${await response.text()}`);
  return response.arrayBuffer();
}

/** GPU capabilities for compressed formats; missing ones fall back to untextured. */
export interface TextureSupport {
  s3tc: boolean;
  s3tcSrgb: boolean;
  rgtc: boolean;
}

export function detectSupport(renderer: THREE.WebGLRenderer): TextureSupport {
  const ext = (name: string) => renderer.extensions.has(name);
  return {
    s3tc: ext("WEBGL_compressed_texture_s3tc"),
    s3tcSrgb: ext("WEBGL_compressed_texture_s3tc_srgb"),
    rgtc: ext("EXT_texture_compression_rgtc"),
  };
}

function createTexture(data: TextureData, support: TextureSupport): THREE.Texture | null {
  const mipmaps = data.mips.map((m) => ({ data: m.data, width: m.width, height: m.height }));
  const first = mipmaps[0];
  if (!first) return null;
  let texture: THREE.Texture;
  switch (data.format) {
    case "bc1":
    case "bc2":
    case "bc3": {
      if (!support.s3tc || (data.srgb && !support.s3tcSrgb)) return null;
      const format = {
        bc1: THREE.RGBA_S3TC_DXT1_Format,
        bc2: THREE.RGBA_S3TC_DXT3_Format,
        bc3: THREE.RGBA_S3TC_DXT5_Format,
      }[data.format];
      texture = new THREE.CompressedTexture(mipmaps, first.width, first.height, format);
      break;
    }
    case "bc4":
    case "bc5": {
      if (!support.rgtc) return null;
      const format = data.format === "bc4" ? THREE.RED_RGTC1_Format : THREE.RED_GREEN_RGTC2_Format;
      texture = new THREE.CompressedTexture(mipmaps, first.width, first.height, format);
      break;
    }
    case "rgba8":
    case "r8": {
      const format = data.format === "rgba8" ? THREE.RGBAFormat : THREE.RedFormat;
      const dataTexture = new THREE.DataTexture(first.data, first.width, first.height, format);
      dataTexture.mipmaps = mipmaps;
      texture = dataTexture;
      break;
    }
  }
  // Game UVs use the D3D convention (v down); uploading without a flip and
  // keeping UVs as stored preserves it.
  texture.flipY = false;
  texture.colorSpace = data.srgb ? THREE.SRGBColorSpace : THREE.NoColorSpace;
  texture.wrapS = data.repeatU ? THREE.RepeatWrapping : THREE.ClampToEdgeWrapping;
  texture.wrapT = data.repeatV ? THREE.RepeatWrapping : THREE.ClampToEdgeWrapping;
  texture.magFilter = THREE.LinearFilter;
  texture.minFilter = mipmaps.length > 1 ? THREE.LinearMipmapLinearFilter : THREE.LinearFilter;
  texture.generateMipmaps = false;
  texture.anisotropy = 4;
  texture.needsUpdate = true;
  return texture;
}

export class AssetCache {
  private models = new Map<string, Promise<ModelAsset>>();
  private textures = new Map<string, Promise<THREE.Texture | null>>();

  constructor(private readonly support: TextureSupport) {}

  /** Loads a model; `parts` limits it to the named parts (empty: all). */
  model(path: string, look: string | null, variant: string | null, parts: string[] = []): Promise<ModelAsset> {
    const partList = parts.length > 0 ? parts.join(",") : null;
    const key = `${path}|${look ?? ""}|${variant ?? ""}|${partList ?? ""}`;
    let entry = this.models.get(key);
    if (!entry) {
      entry = fetchBuffer(assetUrl("model", { path, look, variant, parts: partList }))
        .then(decodeModel)
        .then((data) => this.buildModel(data));
      this.models.set(key, entry);
    }
    return entry;
  }

  private texture(path: string): Promise<THREE.Texture | null> {
    let entry = this.textures.get(path);
    if (!entry) {
      entry = fetchBuffer(assetUrl("texture", { path }))
        .then((buffer) => createTexture(decodeTexture(buffer), this.support))
        .catch((error: unknown) => {
          console.warn(`texture ${path}:`, error);
          return null;
        });
      this.textures.set(path, entry);
    }
    return entry;
  }

  private async buildModel(data: ModelData): Promise<ModelAsset> {
    const materials = await Promise.all(data.materials.map((m) => this.buildMaterial(m)));
    const paintable = materials.filter((_, i) => data.materials[i]?.paintable);
    const meshes = data.pieces.flatMap((piece) => {
      const material = materials[piece.material];
      if (!material) return [];
      const geometry = new THREE.BufferGeometry();
      geometry.setAttribute("position", new THREE.BufferAttribute(piece.positions, 3));
      geometry.setAttribute("normal", new THREE.BufferAttribute(piece.normals, 3));
      if (piece.uvs) geometry.setAttribute("uv", new THREE.BufferAttribute(piece.uvs, 2));
      geometry.setIndex(new THREE.BufferAttribute(piece.indices, 1));
      geometry.computeBoundingSphere();
      const mesh = new THREE.Mesh(geometry, material);
      mesh.renderOrder = data.materials[piece.material]?.decal ? 1 : 0;
      return [mesh];
    });
    return { meshes, paintable };
  }

  private async buildMaterial(header: MaterialHeader): Promise<THREE.MeshStandardMaterial> {
    const map = header.baseTexture ? await this.texture(header.baseTexture) : null;
    const material = new THREE.MeshStandardMaterial({
      // .mat colours are linear, like paint colours.
      color: new THREE.Color().setRGB(...header.diffuse, THREE.LinearSRGBColorSpace),
      map,
      metalness: header.paintable ? 0.35 : 0.1,
      roughness: header.paintable ? 0.35 : 0.6,
      side: THREE.FrontSide,
    });
    if (header.emissive) {
      material.emissive = new THREE.Color(0x222222);
      material.emissiveMap = map;
    }
    switch (header.blend) {
      case "alphaTest":
        material.alphaTest = 0.5;
        break;
      case "blend":
        material.transparent = true;
        material.depthWrite = false;
        if (header.effect.includes("glass")) material.opacity = 0.35;
        if (header.effect.includes("water")) material.opacity = 0.6;
        break;
      case "additive":
        material.transparent = true;
        material.depthWrite = false;
        material.blending = THREE.AdditiveBlending;
        break;
      default:
        break;
    }
    if (header.decal) {
      material.polygonOffset = true;
      material.polygonOffsetFactor = -1;
      material.polygonOffsetUnits = -1;
    }
    return material;
  }
}
