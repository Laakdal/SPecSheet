// Screen-space grouping of hotspot anchors. Many slot locators sit within a
// few centimetres of each other (e.g. on the front face), so markers that
// would overlap are merged into one cluster that lists its slots.

import type { Hotspot } from "./api";
import type { ProjectedAnchor } from "./viewer/TruckViewer";

export interface Cluster {
  key: string;
  x: number;
  y: number;
  hotspots: Hotspot[];
}

/** Greedy clustering: each visible anchor joins the first cluster whose
 * centre is within `radius` pixels, otherwise it starts a new one. */
export function clusterHotspots(
  hotspots: Hotspot[],
  projected: ProjectedAnchor[],
  radius: number,
): Cluster[] {
  const bySlot = new Map(hotspots.map((h) => [h.slot, h]));
  const clusters: { x: number; y: number; members: { hotspot: Hotspot; x: number; y: number }[] }[] = [];
  for (const anchor of projected) {
    const hotspot = bySlot.get(anchor.slot);
    if (!hotspot || !anchor.facing) continue;
    const near = clusters.find((c) => Math.hypot(c.x - anchor.x, c.y - anchor.y) <= radius);
    if (near) {
      near.members.push({ hotspot, x: anchor.x, y: anchor.y });
      near.x = near.members.reduce((s, m) => s + m.x, 0) / near.members.length;
      near.y = near.members.reduce((s, m) => s + m.y, 0) / near.members.length;
    } else {
      clusters.push({ x: anchor.x, y: anchor.y, members: [{ hotspot, x: anchor.x, y: anchor.y }] });
    }
  }
  return clusters.map((c) => {
    const members = c.members.map((m) => m.hotspot).sort((a, b) => a.label.localeCompare(b.label));
    return { key: members.map((m) => m.slot).join("+"), x: c.x, y: c.y, hotspots: members };
  });
}
