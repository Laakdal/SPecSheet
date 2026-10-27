import type { AccessoryOption, Hotspot, SlotEntry, SlotOptions } from "../api";

interface Props {
  hotspots: Hotspot[];
  /** Slots without a marker, shown in the "General" group. */
  slots: SlotEntry[];
  slotOptions: SlotOptions | null;
  busy: boolean;
  onSelectSlot: (slot: string | null) => void;
  onSelectAccessory: (option: AccessoryOption) => void;
}

const CATEGORY_TITLES: Record<Hotspot["category"], string> = {
  base: "Chassis & cabin",
  addon: "Exterior",
  wheel: "Wheels",
  interior: "Interior",
};

/** Right-hand panel: the slot list, or the options of the active slot. */
export function Sidebar({ hotspots, slots, slotOptions, busy, onSelectSlot, onSelectAccessory }: Props) {
  if (slotOptions) {
    return (
      <aside className="sidebar">
        <header className="sidebar-header">
          <button type="button" className="back" onClick={() => onSelectSlot(null)} aria-label="Back to all parts">
            ←
          </button>
          <h2>{slotOptions.label}</h2>
          <span className="count">{slotOptions.options.length}</span>
        </header>
        <ul className="option-list">
          {slotOptions.options.map((option) => (
            <li key={option.id}>
              <button
                type="button"
                className={`option${option.selected ? " is-selected" : ""}${option.compatible ? "" : " is-incompatible"}`}
                disabled={busy || option.selected || !option.compatible}
                title={option.reason ?? option.id}
                onClick={() => onSelectAccessory(option)}
              >
                <span className="option-name">{option.name}</span>
                <span className="option-meta">
                  {option.price !== null && <span>€{option.price.toLocaleString()}</span>}
                  {option.unlockLevel ? <span>Lvl {option.unlockLevel}</span> : null}
                </span>
                {!option.compatible && option.reason && <span className="option-reason">{option.reason}</span>}
              </button>
            </li>
          ))}
        </ul>
      </aside>
    );
  }

  const byLabel = (a: SlotEntry, b: SlotEntry) => a.label.localeCompare(b.label);
  const groups: { title: string; items: SlotEntry[] }[] = [
    ...(["base", "addon", "wheel", "interior"] as const).map((category) => ({
      title: CATEGORY_TITLES[category],
      items: hotspots.filter((h) => h.category === category).sort(byLabel),
    })),
    { title: "General", items: [...slots].sort(byLabel) },
  ];
  return (
    <aside className="sidebar">
      <header className="sidebar-header">
        <h2>Parts</h2>
        <span className="count">{hotspots.length + slots.length}</span>
      </header>
      <div className="slot-groups">
        {groups.map(({ title, items }) =>
          items.length === 0 ? null : (
            <section key={title}>
              <h3>{title}</h3>
              <ul className="slot-list">
                {items.map((h) => (
                  <li key={h.slot}>
                    <button type="button" className="slot" onClick={() => onSelectSlot(h.slot)}>
                      <span className="slot-name">{h.label}</span>
                      <span className="slot-selected">{h.selected ?? "None"}</span>
                    </button>
                  </li>
                ))}
              </ul>
            </section>
          ),
        )}
      </div>
    </aside>
  );
}
