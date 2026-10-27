import { useCallback, useEffect, useState } from "react";
import {
  api,
  errorMessage,
  type AccessoryOption,
  type GameInfo,
  type Scene,
  type SlotOptions,
  type TruckView,
} from "./api";
import { Sidebar } from "./components/Sidebar";
import { Viewport } from "./components/Viewport";

export function App() {
  const [game, setGame] = useState<GameInfo | null>(null);
  const [gameDir, setGameDir] = useState("");
  const [view, setView] = useState<TruckView | null>(null);
  const [slotOptions, setSlotOptions] = useState<SlotOptions | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [discovering, setDiscovering] = useState(true);
  const [loadingModels, setLoadingModels] = useState(false);
  const [assetErrors, setAssetErrors] = useState<string[]>([]);
  const [showInterior, setShowInterior] = useState(false);
  const [scenes, setScenes] = useState<Scene[]>([]);
  const [scene, setScene] = useState<Scene | null>(null);

  const run = useCallback(async <T,>(task: () => Promise<T>): Promise<T | undefined> => {
    setBusy(true);
    setError(null);
    try {
      return await task();
    } catch (e) {
      setError(errorMessage(e));
      return undefined;
    } finally {
      setBusy(false);
    }
  }, []);

  const showGame = useCallback(
    async (info: GameInfo) => {
      setGame(info);
      const gameScenes = (await run(() => api.listScenes())) ?? [];
      setScenes(gameScenes);
      setScene(gameScenes[0] ?? null);
      const first = info.trucks.find((t) => t.id === "scania.s_2016") ?? info.trucks[0];
      if (first) setView((await run(() => api.loadTruck(first.id))) ?? null);
    },
    [run],
  );

  const openGame = async (dir: string) => {
    const info = await run(() => api.openGame(dir));
    if (info) await showGame(info);
  };

  // Discover the installation on start-up.
  useEffect(() => {
    let cancelled = false;
    api
      .openGame()
      .then(async (info) => {
        if (!cancelled) await showGame(info);
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(errorMessage(e));
      })
      .finally(() => {
        if (!cancelled) setDiscovering(false);
      });
    return () => {
      cancelled = true;
    };
  }, [showGame]);

  const selectSlot = useCallback(
    async (slot: string | null) => {
      if (!slot) return setSlotOptions(null);
      setSlotOptions((await run(() => api.slotOptions(slot))) ?? null);
    },
    [run],
  );

  const selectTruck = async (truck: string) => {
    setSlotOptions(null);
    const next = await run(() => api.loadTruck(truck));
    if (next) setView(next);
  };

  const selectAccessory = async (option: AccessoryOption) => {
    const next = await run(() => api.selectAccessory(option.id));
    if (!next) return;
    setView(next);
    if (slotOptions) setSlotOptions((await run(() => api.slotOptions(slotOptions.slot))) ?? null);
  };

  const toggleDrive = async () => {
    if (!view) return;
    const next = await run(() => api.setRightHandDrive(!view.rightHandDrive));
    if (next) setView(next);
  };

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">
          <span className="brand-mark" aria-hidden />
          <span>SpecSheet</span>
        </div>
        {game && (
          <select
            className="truck-select"
            value={view?.truck ?? ""}
            disabled={busy}
            onChange={(e) => void selectTruck(e.target.value)}
            aria-label="Truck model"
          >
            {game.trucks.map((t) => (
              <option key={t.id} value={t.id}>
                {t.name}
              </option>
            ))}
          </select>
        )}
        {view && (
          <button type="button" className="toggle" onClick={() => void toggleDrive()} disabled={busy}>
            {view.rightHandDrive ? "Right-hand drive" : "Left-hand drive"}
          </button>
        )}
        {scenes.length > 0 && (
          <select
            className="truck-select"
            value={scene?.id ?? ""}
            onChange={(e) => setScene(scenes.find((s) => s.id === e.target.value) ?? null)}
            aria-label="Scene"
          >
            {scenes.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name}
              </option>
            ))}
          </select>
        )}
        {view && (
          <label className="toggle">
            <input type="checkbox" checked={showInterior} onChange={(e) => setShowInterior(e.target.checked)} />
            Interior markers
          </label>
        )}
        <div className="status">
          {(busy || loadingModels) && <span className="spinner" aria-label="Loading" />}
          {game && <span className="game-root" title={game.root}>{game.root}</span>}
        </div>
      </header>

      {!game && !busy && !discovering && (
        <div className="locate">
          <p>{error ?? "Locating Euro Truck Simulator 2…"}</p>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void openGame(gameDir);
            }}
          >
            <input
              value={gameDir}
              onChange={(e) => setGameDir(e.target.value)}
              placeholder="C:\\Steam\\steamapps\\common\\Euro Truck Simulator 2"
              aria-label="Game directory"
            />
            <button type="submit">Open</button>
          </form>
        </div>
      )}

      <main className="workspace">
        <Viewport
          view={view}
          scene={scene}
          showInterior={showInterior}
          activeSlot={slotOptions?.slot ?? null}
          onSelectSlot={(slot) => void selectSlot(slot)}
          onLoadingChange={setLoadingModels}
          onAssetErrors={setAssetErrors}
        />
        {view && (
          <Sidebar
            hotspots={view.hotspots}
            slots={view.slots}
            slotOptions={slotOptions}
            busy={busy}
            onSelectSlot={(slot) => void selectSlot(slot)}
            onSelectAccessory={(option) => void selectAccessory(option)}
          />
        )}
      </main>

      {(error || assetErrors.length > 0 || (view?.warnings.length ?? 0) > 0) && game && (
        <footer className="messages">
          {error && <p className="error">{error}</p>}
          {[...(view?.warnings ?? []), ...assetErrors].slice(0, 3).map((w) => (
            <p key={w} className="warning">
              {w}
            </p>
          ))}
        </footer>
      )}
    </div>
  );
}
