// Typed client for the Tauri backend. Types mirror app/src-tauri/src/dto.rs.

import { invoke } from "@tauri-apps/api/core";

export type Vec3 = [number, number, number];

export interface TruckEntry {
  id: string;
  name: string;
}

export interface GameInfo {
  root: string;
  trucks: TruckEntry[];
  warnings: string[];
}

export interface Placement {
  slot: string;
  accessory: string;
  model: string;
  look: string | null;
  variant: string | null;
  translation: Vec3;
  /** Quaternion (x, y, z, w). */
  rotation: [number, number, number, number];
  scale: number;
}

export type HotspotCategory = "base" | "addon" | "wheel" | "interior";

export interface Hotspot {
  slot: string;
  label: string;
  /** Truck space: x right, y up, -z forward (same as three.js). */
  position: Vec3;
  category: HotspotCategory;
  selected: string | null;
  optionCount: number;
}

/** A slot without a marker (paint job, engine, interior-only parts...). */
export interface SlotEntry {
  slot: string;
  label: string;
  selected: string | null;
  optionCount: number;
}

export interface TruckView {
  truck: string;
  name: string;
  rightHandDrive: boolean;
  placements: Placement[];
  hotspots: Hotspot[];
  slots: SlotEntry[];
  paintColor: Vec3 | null;
  warnings: string[];
}

/** A tuning scene from /def/desktop/config*.sii; null fields are unspecified. */
export interface Scene {
  id: string;
  name: string;
  model: { path: string; look: string | null; variant: string | null; position: Vec3; parts: string[] } | null;
  camera: { position: Vec3; aim: Vec3; fov: number } | null;
  lightDirection: Vec3 | null;
  lightAmbient: Vec3 | null;
  lightDiffuse: Vec3 | null;
  lightEnvironment: number | null;
  fogColor: Vec3 | null;
  fogDensity: number | null;
  truckPosition: Vec3;
  /** Euler angles in degrees. */
  truckRotation: Vec3;
}

export interface AccessoryOption {
  id: string;
  name: string;
  price: number | null;
  unlockLevel: number | null;
  selected: boolean;
  compatible: boolean;
  reason: string | null;
}

export interface SlotOptions {
  slot: string;
  label: string;
  options: AccessoryOption[];
}

export const api = {
  openGame: (dir?: string) => invoke<GameInfo>("open_game", { dir: dir ?? null }),
  loadTruck: (truck: string) => invoke<TruckView>("load_truck", { truck }),
  slotOptions: (slot: string) => invoke<SlotOptions>("slot_options", { slot }),
  selectAccessory: (accessory: string) => invoke<TruckView>("select_accessory", { accessory }),
  setRightHandDrive: (enabled: boolean) => invoke<TruckView>("set_right_hand_drive", { enabled }),
  listScenes: () => invoke<Scene[]>("list_scenes"),
};

/** URL of an asset served by the backend's read-only `ets2://` protocol. */
export function assetUrl(endpoint: "model" | "texture", params: Record<string, string | null>): string {
  const query = Object.entries(params)
    .filter((entry): entry is [string, string] => entry[1] !== null)
    .map(([key, value]) => `${key}=${encodeURIComponent(value)}`)
    .join("&");
  // WebView2 (Windows) exposes custom schemes as http://<scheme>.localhost.
  const base = navigator.userAgent.includes("Windows") ? "http://ets2.localhost" : "ets2://localhost";
  return `${base}/${endpoint}?${query}`;
}

export function errorMessage(error: unknown): string {
  return typeof error === "string" ? error : error instanceof Error ? error.message : String(error);
}
