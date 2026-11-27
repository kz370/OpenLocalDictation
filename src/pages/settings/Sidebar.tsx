import { useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { Search } from "lucide-react";

export interface NavItem<Id extends string> {
  id: Id;
  label: string;
  icon: ReactNode;
  /** Shows an amber dot: something here needs the user. */
  attention?: string;
}

export interface SearchEntry<Id extends string> {
  section: Id;
  label: string;
  keywords: string;
}

interface Props<Id extends string> {
  items: NavItem<Id>[];
  active: Id;
  onSelect: (id: Id) => void;
  index: SearchEntry<Id>[];
  /** Opens a section and highlights the row with this label. */
  onJump: (section: Id, label: string) => void;
  sectionLabel: (id: Id) => string;
  shortcut: string;
  status: string;
}

/** Left navigation: brand, settings search, the sections, and the shortcut and version at the foot. */
export function Sidebar<Id extends string>({ items, active, onSelect, index, onJump, sectionLabel, shortcut, status }: Props<Id>) {
  const [query, setQuery] = useState("");
  const navRef = useRef<HTMLElement>(null);
  const q = query.trim().toLowerCase();
  const hits = useMemo(() => (q ? index.filter((e) => `${e.label} ${e.keywords}`.toLowerCase().includes(q)).slice(0, 8) : []), [index, q]);

  const onKeyDown = (e: KeyboardEvent) => {
    const buttons = [...(navRef.current?.querySelectorAll<HTMLButtonElement>(".nav-item") ?? [])];
    const i = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (i < 0) return;
    const to = e.key === "ArrowDown" ? (i + 1) % buttons.length : e.key === "ArrowUp" ? (i - 1 + buttons.length) % buttons.length : e.key === "Home" ? 0 : e.key === "End" ? buttons.length - 1 : -1;
    if (to < 0) return;
    e.preventDefault();
    buttons[to].focus();
  };

  return (
    <aside className="sidebar">
      <div className="brand">
        <span className="brand-mark" aria-hidden>
          <img src="/app-icon.png" alt="" />
        </span>
        <span className="brand-name">Open Local Dictation</span>
      </div>
      <div className="sidebar-search">
        <Search size={13} aria-hidden />
        <input
          className="input"
          type="search"
          placeholder="Search settings…"
          aria-label="Search settings"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") setQuery("");
            if (e.key === "Enter" && hits[0]) {
              onJump(hits[0].section, hits[0].label);
              setQuery("");
            }
          }}
        />
      </div>
      {q ? (
        <div className="search-results" role="listbox" aria-label="Search results">
          {hits.length === 0 && <span className="row-hint" style={{ padding: "6px 10px" }}>No setting matches.</span>}
          {hits.map((h) => (
            <button
              key={`${h.section}-${h.label}`}
              type="button"
              role="option"
              aria-selected={false}
              className="search-hit"
              onClick={() => {
                onJump(h.section, h.label);
                setQuery("");
              }}
            >
              {h.label}
              <small>{sectionLabel(h.section)}</small>
            </button>
          ))}
        </div>
      ) : (
        <nav className="nav" aria-label="Settings sections" ref={navRef} onKeyDown={onKeyDown}>
          {items.map((it, n) => (
            <button
              key={it.id}
              type="button"
              className="nav-item"
              aria-current={active === it.id ? "page" : undefined}
              title={it.attention ? `${it.label} — ${it.attention}` : `${it.label} (Ctrl+${n + 1})`}
              onClick={() => onSelect(it.id)}
            >
              {it.icon}
              <span className="nav-label">{it.label}</span>
              {it.attention && <span className="nav-flag" role="img" aria-label={it.attention} />}
            </button>
          ))}
        </nav>
      )}
      <div className="sidebar-foot">
        <span className="kbd-chip" title="Dictation shortcut">
          {shortcut}
        </span>
        <span className="foot-text">{status}</span>
      </div>
    </aside>
  );
}
