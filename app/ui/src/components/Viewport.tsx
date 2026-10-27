import { useEffect, useMemo, useRef, useState } from "react";
import type { Hotspot, Scene, TruckView } from "../api";
import { clusterHotspots, type Cluster } from "../hotspots";
import { TruckViewer, type ProjectedAnchor } from "../viewer/TruckViewer";

const CLUSTER_RADIUS_PX = 22;

interface Props {
  view: TruckView | null;
  scene: Scene | null;
  showInterior: boolean;
  activeSlot: string | null;
  onSelectSlot: (slot: string) => void;
  onLoadingChange: (loading: boolean) => void;
  onAssetErrors: (errors: string[]) => void;
}

export function Viewport({ view, scene, showInterior, activeSlot, onSelectSlot, onLoadingChange, onAssetErrors }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewerRef = useRef<TruckViewer | null>(null);
  const [projected, setProjected] = useState<ProjectedAnchor[]>([]);
  const [openCluster, setOpenCluster] = useState<string | null>(null);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const viewer = new TruckViewer(container);
    viewer.onProjection(setProjected);
    viewerRef.current = viewer;
    return () => {
      viewer.dispose();
      viewerRef.current = null;
    };
  }, []);

  useEffect(() => {
    const viewer = viewerRef.current;
    if (!viewer || !view) return;
    let cancelled = false;
    onLoadingChange(true);
    viewer
      .show(view)
      .then((errors) => !cancelled && onAssetErrors(errors))
      .finally(() => !cancelled && onLoadingChange(false));
    return () => {
      cancelled = true;
    };
  }, [view, onLoadingChange, onAssetErrors]);

  useEffect(() => {
    const viewer = viewerRef.current;
    if (!viewer || !scene) return;
    let cancelled = false;
    void viewer.setScene(scene).then((errors) => {
      if (!cancelled && errors.length > 0) onAssetErrors(errors);
    });
    return () => {
      cancelled = true;
    };
  }, [scene, onAssetErrors]);

  const clusters = useMemo(() => {
    const visible = (view?.hotspots ?? []).filter((h) => showInterior || h.category !== "interior");
    return clusterHotspots(visible, projected, CLUSTER_RADIUS_PX);
  }, [view, projected, showInterior]);

  const pick = (hotspot: Hotspot) => {
    setOpenCluster(null);
    onSelectSlot(hotspot.slot);
  };

  return (
    <div className="viewport">
      <div ref={containerRef} className="viewport-canvas" onPointerDown={() => setOpenCluster(null)} />
      <div className="hotspot-layer">
        {clusters.map((cluster) => (
          <ClusterMarker
            key={cluster.key}
            cluster={cluster}
            active={cluster.hotspots.some((h) => h.slot === activeSlot)}
            open={openCluster === cluster.key}
            onToggle={() => setOpenCluster(openCluster === cluster.key ? null : cluster.key)}
            onPick={pick}
          />
        ))}
      </div>
    </div>
  );
}

function ClusterMarker({
  cluster,
  active,
  open,
  onToggle,
  onPick,
}: {
  cluster: Cluster;
  active: boolean;
  open: boolean;
  onToggle: () => void;
  onPick: (hotspot: Hotspot) => void;
}) {
  const single = cluster.hotspots.length === 1 ? cluster.hotspots[0] : undefined;
  const category = single?.category ?? "group";
  return (
    <div className="hotspot-anchor" style={{ transform: `translate(${cluster.x}px, ${cluster.y}px)` }}>
      <button
        type="button"
        className={`hotspot hotspot-${category}${active ? " is-active" : ""}`}
        aria-label={single ? single.label : `${cluster.hotspots.length} parts`}
        onClick={() => (single ? onPick(single) : onToggle())}
      >
        {single ? null : cluster.hotspots.length}
      </button>
      {single && <span className="hotspot-label">{single.label}</span>}
      {open && !single && (
        <ul className="cluster-menu" role="menu">
          {cluster.hotspots.map((h) => (
            <li key={h.slot}>
              <button type="button" role="menuitem" onClick={() => onPick(h)}>
                <span>{h.label}</span>
                {h.selected && <small>{h.selected}</small>}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
