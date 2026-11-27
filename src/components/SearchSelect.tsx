import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { Check, ChevronDown, Search } from "lucide-react";

export interface SearchOption {
  value: string;
  label: string;
  /** Extra text that also matches the search (native name, code). */
  keywords?: string;
}

/** Compact dropdown with a filter box: a short list instead of a screen-tall native select. */
export function SearchSelect(props: { value: string; options: SearchOption[]; onChange: (v: string) => void; label: string; placeholder?: string; /** Show the filter box; by default only for lists longer than four. */ searchable?: boolean }) {
  const searchable = props.searchable ?? props.options.length > 4;
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const root = useRef<HTMLDivElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  // The menu is fixed to the viewport, so a card that scrolls or clips its content cannot hide it.
  const [place, setPlace] = useState<React.CSSProperties>({});
  const list = useRef<HTMLDivElement>(null);

  const current = props.options.find((o) => o.value === props.value);
  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    return q ? props.options.filter((o) => `${o.label} ${o.keywords ?? ""} ${o.value}`.toLowerCase().includes(q)) : props.options;
  }, [props.options, query]);

  const close = () => {
    setOpen(false);
    setQuery("");
  };
  const choose = (v: string) => {
    close();
    if (v !== props.value) props.onChange(v);
  };

  useEffect(() => {
    if (!open) return;
    const onDown = (e: PointerEvent) => !root.current?.contains(e.target as Node) && close();
    document.addEventListener("pointerdown", onDown);
    return () => document.removeEventListener("pointerdown", onDown);
  }, [open]);

  useLayoutEffect(() => {
    if (!open || !root.current) return;
    const r = root.current.getBoundingClientRect();
    const need = 260; // search box + the list's 168 px + padding
    const below = window.innerHeight - r.bottom - 8;
    const flip = below < need && r.top - 8 > below;
    setPlace({
      position: "fixed",
      insetInlineStart: r.left,
      insetInlineEnd: "auto",
      // As wide as the longest option (not as wide as the button); the filter box keeps a usable minimum.
      width: "max-content",
      minWidth: searchable ? 180 : 0,
      maxWidth: Math.max(160, Math.min(420, window.innerWidth - r.left - 8)),
      ...(flip ? { top: "auto", bottom: window.innerHeight - r.top + 4 } : { top: r.bottom + 4 }),
    });
    // A fixed menu does not follow its button: close when the page scrolls or resizes.
    const onMove = (e: Event) => {
      if (e.target instanceof Node && menuRef.current?.contains(e.target)) return;
      // Only when the button really moved: a scroll that leaves it where it is
      // (the card around it nudging itself as the menu opens) must not close
      // the menu it just opened, which showed as a click that did nothing.
      const now = root.current?.getBoundingClientRect();
      if (now && Math.abs(now.left - r.left) < 1 && Math.abs(now.top - r.top) < 1) return;
      close();
    };
    window.addEventListener("scroll", onMove, true);
    window.addEventListener("resize", onMove);
    return () => {
      window.removeEventListener("scroll", onMove, true);
      window.removeEventListener("resize", onMove);
    };
  }, [open]);

  useEffect(() => setActive(0), [query, open]);
  useEffect(() => {
    // Scrolls the list itself. scrollIntoView would also scroll the cards and pages around the (fixed) menu.
    const box = list.current;
    const row = box?.querySelector<HTMLElement>('[data-active="true"]');
    if (!box || !row) return;
    if (row.offsetTop < box.scrollTop) box.scrollTop = row.offsetTop;
    else if (row.offsetTop + row.offsetHeight > box.scrollTop + box.clientHeight) box.scrollTop = row.offsetTop + row.offsetHeight - box.clientHeight;
  }, [active, open]);

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      close();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((i) => Math.min(shown.length - 1, i + 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((i) => Math.max(0, i - 1));
    } else if (e.key === "Enter" && shown[active]) {
      e.preventDefault();
      choose(shown[active].value);
    }
  };

  return (
    <div className="ss" ref={root}>
      <button type="button" className="input ss-button" aria-label={props.label} aria-haspopup="listbox" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        <span className="ss-current">
          <span className="ss-text">{current?.label ?? props.value}</span>
          {/* Every option, invisible and stacked in the same cell: the button is as wide as the longest one, whichever is picked. */}
          {props.options.map((o) => (<span key={o.value} className="ss-size" aria-hidden>{o.label}</span>))}
        </span>
        <ChevronDown size={14} aria-hidden />
      </button>
      {open && (
        <div className="ss-menu" style={place} onKeyDown={onKey} tabIndex={-1} ref={(el) => {
            menuRef.current = el;
            // preventScroll: focusing would otherwise scroll the card around the (fixed) menu.
            if (!searchable) el?.focus({ preventScroll: true });
          }}>
          {searchable && (
          <div className="ss-search">
            <Search size={13} aria-hidden />
            <input ref={(el) => { if (el && document.activeElement !== el) el.focus({ preventScroll: true }); }} value={query} placeholder={props.placeholder ?? "Search…"} aria-label={`${props.label}: search`} onChange={(e) => setQuery(e.target.value)} />
          </div>
          )}
          <div className="ss-list" role="listbox" aria-label={props.label} ref={list}>
            {shown.length === 0 && <div className="ss-empty">No match</div>}
            {shown.map((o, i) => (
              <button
                key={o.value}
                type="button"
                role="option"
                aria-selected={o.value === props.value}
                data-active={i === active}
                className="ss-option"
                onMouseEnter={() => setActive(i)}
                onClick={() => choose(o.value)}
              >
                <span className="ss-check">{o.value === props.value && <Check size={12} />}</span>
                <span>{o.label}</span>
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
