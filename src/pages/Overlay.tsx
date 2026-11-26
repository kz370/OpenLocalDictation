import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { Check, CheckCircle2, ChevronDown, Copy, Globe, Maximize2, Mic, Minimize2, Pencil, Pin, PinOff, RotateCcw, Settings as SettingsIcon, Square, X, XCircle } from "lucide-react";
import { getCurrentWindow, PhysicalPosition } from "@tauri-apps/api/window";
import { ipc, on } from "../ipc";
import { KNOWN_LANGUAGES } from "../knownLanguages";
import { useSettings } from "../settingsStore";
import { errorMessage, t } from "../strings";
import type { DictationStateEvent, DictationTargetEvent, VoiceEvent } from "../types";
import { textDir } from "../components/controls";
import { VoiceBars } from "../components/VoiceBars";
import { TooltipLayer } from "../components/Tooltip";
import { prettyAccelerator } from "../components/ShortcutInput";

/** Free drag on the overlay's header strip; Shift locks to horizontal-only,
 * Alt locks to vertical-only. Uses manual `setPosition` (not Tauri's
 * `startDragging`, which hands off to the OS and can't be axis-constrained
 * mid-drag), throttled to one IPC call per frame. */
function useOverlayDrag(horizontalOnly: boolean, locked: boolean) {
  const drag = useRef<{ startX: number; startY: number; winX: number; winY: number; frame: number | null; pending: { x: number; y: number } | null } | null>(null);

  const flush = () => {
    const d = drag.current;
    if (!d) return;
    d.frame = null;
    if (!d.pending) return;
    const { x, y } = d.pending;
    d.pending = null;
    void getCurrentWindow().setPosition(new PhysicalPosition(x, y));
  };

  return {
    onPointerDown: (e: React.PointerEvent) => {
      if (e.button !== 0 || locked) return;
      // Without capture, the browser stops delivering move events to this
      // element the instant the (async) window move puts the cursor even
      // slightly outside its bounds — the drag would visually never start.
      e.currentTarget.setPointerCapture(e.pointerId);
      void getCurrentWindow()
        .outerPosition()
        .then((pos) => {
          drag.current = { startX: e.screenX, startY: e.screenY, winX: pos.x, winY: pos.y, frame: null, pending: null };
        });
    },
    onPointerMove: (e: React.PointerEvent) => {
      const d = drag.current;
      if (!d || (e.buttons & 1) === 0) return;
      // screenX/screenY are logical (CSS) pixels; PhysicalPosition wants
      // real screen pixels, so scale by the monitor's DPI factor.
      const scale = window.devicePixelRatio || 1;
      let dx = Math.round((e.screenX - d.startX) * scale);
      let dy = Math.round((e.screenY - d.startY) * scale);
      if (e.shiftKey || horizontalOnly) dy = 0;
      if (e.altKey && !horizontalOnly) dx = 0;
      d.pending = { x: d.winX + dx, y: d.winY + dy };
      if (d.frame == null) d.frame = requestAnimationFrame(flush);
    },
    onPointerUp: (e: React.PointerEvent) => {
      if (e.currentTarget.hasPointerCapture(e.pointerId)) e.currentTarget.releasePointerCapture(e.pointerId);
      drag.current = null;
    },
  };
}

const MINI_MIN_W = 320; // keep in sync with MINI_MIN_WIDTH / MINI_MAX_WIDTH in settings.rs
const MINI_MAX_W = 800;
const MINI_DEFAULT_W = 400;
/** Small card margins (8 + 8), border (1 + 1) and padding (12 + 10). */
const PILL_CHROME = 16 + 2 + 22;
/** Gap between header items of the small pill (.overlay-head gap). */
const PILL_GAP = 8;

/** Width a row of header items needs: every item at its natural width, the
 *  status label at its CSS floor, plus the gaps between them. */
function rowWidth(row: Element | undefined): number {
  if (!row) return 0;
  const px = (v: string) => parseFloat(v) || 0;
  const items = (Array.from(row.children) as HTMLElement[]).filter((el) => {
    const s = getComputedStyle(el);
    return s.display !== "none" && s.position !== "absolute" && s.position !== "fixed";
  });
  let sum = PILL_GAP * Math.max(0, items.length - 1);
  for (const el of items) {
    const s = getComputedStyle(el);
    sum += px(s.flexShrink) > 0 ? px(s.minWidth) : el.getBoundingClientRect().width;
  }
  return sum;
}

/** Narrowest window (logical px) that holds the small pill's header without
 *  squeezing anything, plus the card's chrome. The bubble's pill keeps its
 *  status centered, so both sides get the room of the wider one. */
function pillMinWidth(head: HTMLElement): number {
  // Constants, not computed styles: those are mid-animation while the pill
  // grows. Must match .overlay-card.mini / .overlay-head.
  if (!head.classList.contains("split")) return Math.ceil(rowWidth(head) + PILL_CHROME);
  const [start, center, end] = Array.from(head.children);
  const side = Math.max(rowWidth(start), rowWidth(end));
  return Math.ceil(2 * side + rowWidth(center) + 2 * PILL_GAP + PILL_CHROME);
}

/** Drag handle on the small pill's right edge: width only. The window follows
 *  live; the width is saved when the drag ends. Double-click restores the
 *  default width. Never narrower than the header content (`minWidth`). */
function ResizeEdge(props: { minWidth: () => number }) {
  const drag = useRef<{ x: number; w: number; frame: number | null; next: number } | null>(null);
  const floor = () => Math.max(MINI_MIN_W, props.minWidth());
  const send = (w: number, commit: boolean) => void ipc.overlayResize(Math.max(floor(), w), commit).catch(quiet);
  return (
    <div
      className="overlay-resize"
      title="Drag to resize · double-click to reset"
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        e.currentTarget.setPointerCapture(e.pointerId);
        drag.current = { x: e.screenX, w: window.innerWidth, frame: null, next: window.innerWidth };
      }}
      onPointerMove={(e) => {
        const d = drag.current;
        if (!d || (e.buttons & 1) === 0) return;
        d.next = Math.min(MINI_MAX_W, Math.max(floor(), Math.round(d.w + e.screenX - d.x)));
        if (d.frame == null) {
          d.frame = requestAnimationFrame(() => {
            if (drag.current) drag.current.frame = null;
            send(d.next, false);
          });
        }
      }}
      onPointerUp={(e) => {
        const d = drag.current;
        drag.current = null;
        if (e.currentTarget.hasPointerCapture(e.pointerId)) e.currentTarget.releasePointerCapture(e.pointerId);
        if (d) send(d.next, true);
      }}
      onDoubleClick={() => send(MINI_DEFAULT_W, true)}
    />
  );
}

/** Small header button. Stops the pointer so pressing it never starts a drag. */
function HeadBtn(props: { label: string; onClick: () => void; children: React.ReactNode; disabled?: boolean }) {
  return (
    <button
      type="button"
      className="icon-btn"
      style={{ width: 24, height: 24 }}
      aria-label={props.label}
      title={props.label}
      disabled={props.disabled}
      onPointerDown={(e) => e.stopPropagation()}
      onClick={props.onClick}
    >
      {props.children}
    </button>
  );
}

const quiet = () => undefined;

const HOVER_OPEN_MS = 120; // hover on the bubble before its pill shows (a pass-by does nothing)
const HOVER_CLOSE_MS = 600; // grace after the pointer leaves before the pill hides again
const PILL_FADE_MS = 200; // the pill going back into the circle (.overlay-pill transition) plus a frame
/** Bubble style geometry (logical px); must match .overlay-card.bubble and
 *  .overlay-pill in app.css. The circle's top is ORB_TOP below the card box's
 *  top; the pill sits ORB_GAP above the circle. */
const ORB = 44;
const ORB_TOP = 12;
const ORB_GAP = 6;
const PILL_H = 52;

/** The overlay's mic button: starts dictation when idle, stops and inserts
 *  while listening. Pressing it never starts a drag. */
function MicBtn(props: { live: boolean; onClick: () => void }) {
  const label = t(props.live ? "overlay.micStop" : "overlay.micStart");
  return (
    <button
      type="button"
      className={`overlay-mic${props.live ? " live" : ""}`}
      aria-label={label}
      title={label}
      onPointerDown={(e) => e.stopPropagation()}
      onDoubleClick={(e) => e.stopPropagation()}
      onClick={props.onClick}
    >
      {props.live ? <Square size={11} fill="currentColor" /> : <Mic size={13} />}
    </button>
  );
}

const MENU_ROW = 32; // option height + gap
const MENU_PAD = 12; // padding + border
const MENU_MAX_ROWS = 6;
const MENU_GAP = 6; // between the pill and the menu
const MENU_W = 200; // matches .overlay-lang-menu
const MENU_EDGE = 8; // least room between the menu and the window's edge (its shadow)
const MENU_LEAVE_MS = 500; // grace before a pointer that left the overlay closes the menu

/** The window has one menu rectangle (MENU_RECT in window.rs), so the pill
 *  that put it there is tracked here: only that pill may take it away, which
 *  keeps a close-then-open pair of calls from arriving in the wrong order. */
let menuOwner: object | null = null;

type PillOption = { value: string; short: string; name: string };

/** Header pill with a dropdown list, used for the language and the profile.
 *  The overlay window keeps invisible room above and below the card (see
 *  MENU_ROOM in window.rs) and is clipped to the card; opening the menu adds
 *  the menu's outline to the clip, so the list hangs outside the card at full
 *  size without the window moving. It opens downward, or upward when the
 *  screen ends below. Clicks outside the card and menu go to the app behind,
 *  so leaving them also closes the menu. */
function PillMenu(props: {
  label: string;
  icon: React.ReactNode;
  value: string;
  options: PillOption[];
  onChange: (value: string) => void;
  resetKey: string;
  /** Adds a filter box on top of the options, for lists too long to scan. */
  searchable?: boolean;
  searchPlaceholder?: string;
  /** Lets the page open the menu (the pill's right click). */
  openRef?: React.MutableRefObject<(() => void) | null>;
  onOpenChange?: (open: boolean) => void;
}) {
  const [open, setOpen] = useState<{ top: number; left: number } | null>(null);
  const [query, setQuery] = useState("");
  const ref = useRef<HTMLDivElement>(null);
  const btnRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const openRef = useRef(false);
  const owner = useRef({});
  const onOpenChangeRef = useRef(props.onOpenChange);
  onOpenChangeRef.current = props.onOpenChange;

  const current = props.options.find((o) => o.value === props.value) ?? props.options[0];
  const needle = query.trim().toLowerCase();
  const shown = !needle ? props.options : props.options.filter((o) => `${o.short} ${o.name}`.toLowerCase().includes(needle));
  const rows = shown.length + (props.searchable ? 1 : 0); // the search box is a row
  const menuH = Math.min(rows, MENU_MAX_ROWS) * MENU_ROW + MENU_PAD;

  const close = () => {
    if (!openRef.current) return;
    openRef.current = false;
    setOpen(null);
    setQuery("");
    if (menuOwner !== owner.current) return;
    menuOwner = null;
    void ipc.dictationOverlayMenu(null).catch(quiet);
  };

  const show = async () => {
    const r = btnRef.current?.getBoundingClientRect();
    if (!r || openRef.current) return;
    openRef.current = true;
    const scr = window.screen as Screen & { availTop?: number };
    const screenBottom = (scr.availTop ?? 0) + scr.availHeight;
    const down = window.screenY + r.bottom + MENU_GAP + menuH <= screenBottom;
    const top = down ? r.bottom + MENU_GAP : r.top - MENU_GAP - menuH;
    // Hang from the chip's outer side (left-aligned when the chip is on the
    // window's left half, right-aligned otherwise), always inside the window.
    const winW = document.documentElement.clientWidth;
    const anchored = r.left + r.width / 2 < winW / 2 ? r.left : r.right - MENU_W;
    const left = Math.max(MENU_EDGE, Math.min(anchored, winW - MENU_W - MENU_EDGE));
    // Unclip the menu's area first, so its first frame isn't cut off.
    menuOwner = owner.current;
    await ipc.dictationOverlayMenu([left, top, MENU_W, menuH]).catch(quiet);
    if (!openRef.current) return;
    setOpen({ top, left });
  };

  useEffect(() => {
    onOpenChangeRef.current?.(open !== null);
  }, [open]);

  useEffect(() => {
    if (!open) return;
    menuRef.current?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: "nearest" });
    const onDown = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) close();
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && close();
    let leaveTimer: number | undefined;
    const onLeave = () => (leaveTimer = window.setTimeout(close, MENU_LEAVE_MS));
    const onEnter = () => window.clearTimeout(leaveTimer);
    const root = document.documentElement;
    document.addEventListener("pointerdown", onDown);
    document.addEventListener("keydown", onKey);
    root.addEventListener("mouseleave", onLeave);
    root.addEventListener("mouseenter", onEnter);
    return () => {
      window.clearTimeout(leaveTimer);
      document.removeEventListener("pointerdown", onDown);
      document.removeEventListener("keydown", onKey);
      root.removeEventListener("mouseleave", onLeave);
      root.removeEventListener("mouseenter", onEnter);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  // A new dictation state (review, done) or unmounting covers the room again.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(() => close, [props.resetKey]);

  if (props.openRef) props.openRef.current = () => void show();

  const up = open !== null && open.top < (btnRef.current?.getBoundingClientRect().top ?? 0);
  return (
    <div ref={ref} className="overlay-lang-wrap" onPointerDown={(e) => e.stopPropagation()} onDoubleClick={(e) => e.stopPropagation()}>
      <button
        ref={btnRef}
        type="button"
        className={`overlay-lang${open ? " open" : ""}${up ? " up" : ""}`}
        title={`${props.label}: ${current.name}`}
        aria-label={`${props.label}: ${current.name}`}
        aria-haspopup="listbox"
        aria-expanded={!!open}
        onClick={() => (open ? close() : void show())}
      >
        {props.icon}
        {current.short && <span className="overlay-lang-current">{current.short}</span>}
        <ChevronDown size={12} aria-hidden className="overlay-lang-chevron" />
      </button>
      {open && (
        <div ref={menuRef} className="overlay-lang-menu" role="listbox" aria-label={props.label} style={{ top: open.top, left: open.left, maxHeight: menuH }}>
          {props.searchable && (
            <input
              className="input overlay-lang-search"
              autoFocus
              value={query}
              placeholder={props.searchPlaceholder}
              aria-label={`${props.label}: ${t("app.search")}`}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key !== "Enter" || !shown.length) return;
                e.preventDefault();
                close();
                if (shown[0].value !== props.value) props.onChange(shown[0].value);
              }}
              onPointerDown={(e) => e.stopPropagation()}
            />
          )}
          {shown.map((o) => (
            <button
              key={o.value}
              type="button"
              role="option"
              aria-selected={o.value === props.value}
              className="overlay-lang-option"
              title={o.name}
              onClick={() => {
                close();
                if (o.value !== props.value) props.onChange(o.value);
              }}
            >
              <span className="overlay-lang-check" aria-hidden>
                {o.value === props.value && <Check size={11} />}
              </span>
              <span className="overlay-lang-code">{o.short}</span>
              <span className="overlay-lang-name">{o.name}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

/** Window shown while dictating into another application. Displays what you
 *  say live, then confirms the insertion. In review mode it holds the result
 *  as editable text until you insert or discard it. */
export function Overlay() {
  const [state, setState] = useState<DictationStateEvent["state"]>("idle");
  const [error, setError] = useState<string | null>(null);
  const [text, setText] = useState("");
  const [draft, setDraft] = useState("");
  const [targetApp, setTargetApp] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [reviewPreview, setReviewPreview] = useState(false);
  const [bands, setBands] = useState<number[]>([]);
  const [level, setLevel] = useState(0);
  const [langOpen, setLangOpen] = useState(false);
  const openLang = useRef<(() => void) | null>(null);
  // Tooltips of the overlay show inside its header (see TooltipLayer `inline`): the card sits on the taskbar, so a bubble has no room.
  const [hint, setHint] = useState<string | null>(null);
  const mode = useSettings((s) => s.settings?.mode ?? "hold");
  const reviewBeforeInsert = useSettings((s) => s.settings?.reviewBeforeInsert ?? true);
  const pinned = useSettings((s) => s.settings?.overlayPinned ?? false);
  const showSettingsBtn = useSettings((s) => s.settings?.overlayShowSettings ?? true);
  const opacity = useSettings((s) => s.settings?.overlayOpacity ?? 100);
  const layouts = useSettings((s) => s.settings?.overlayLayouts ?? "small");
  const miniChoice = useSettings((s) => s.settings?.overlayMini ?? false);
  const mini = layouts === "small" ? true : layouts === "large" ? false : miniChoice;
  const sizeLocked = useSettings((s) => s.settings?.overlaySizeLocked ?? false);
  const overlayStyle = useSettings((s) => s.settings?.overlayStyle ?? "pill");
  const shortcut = useSettings((s) => s.settings?.shortcut ?? "");
  const textRef = useRef<HTMLDivElement>(null);
  const editRef = useRef<HTMLTextAreaElement>(null);
  const previewRef = useRef(false);
  const reviewDoneRef = useRef(false);
  const reviewReadyRef = useRef(false);
  const draftRef = useRef("");
  const draftDirty = useRef(false);
  const horizontalOnly = useSettings((s) => s.settings?.overlayHorizontalOnly ?? true);
  const positionLocked = useSettings((s) => s.settings?.overlayPositionLocked ?? false);
  const drag = useOverlayDrag(horizontalOnly, positionLocked);
  const headRef = useRef<HTMLDivElement>(null);
  // Small idle pill folds to a circle; a short hover unfolds it.
  const [hover, setHover] = useState(false);
  const hoverTimer = useRef<number | undefined>(undefined);
  const hoverIn = () => {
    window.clearTimeout(hoverTimer.current);
    hoverTimer.current = window.setTimeout(() => setHover(true), HOVER_OPEN_MS);
  };
  const hoverOut = () => {
    window.clearTimeout(hoverTimer.current);
    hoverTimer.current = window.setTimeout(() => setHover(false), HOVER_CLOSE_MS);
  };
  useEffect(() => () => window.clearTimeout(hoverTimer.current), []);

  useEffect(() => {
    document.documentElement.classList.add("overlay");
    const subs = [
      on<DictationTargetEvent>("dictation://target", (e) => {
        setTargetApp(e.targetApp);
      }),
      on<DictationStateEvent>("dictation://state", (e) => {
        if (e.state === "idle" && (previewRef.current || reviewDoneRef.current)) return;
        if (e.state === "reviewing" && reviewDoneRef.current) return;
        setState(e.state);
        if (e.targetApp !== undefined) setTargetApp(e.targetApp);
        reviewReadyRef.current = e.state === "review";
        if (e.state === "listening") {
          setText("");
          setError(null);
          setBands([]);
          setLevel(0);
          setTargetApp(null);
          setCopied(false);
          previewRef.current = false;
          reviewDoneRef.current = false;
          draftRef.current = "";
          draftDirty.current = false;
          setReviewPreview(false);
        }
        if (e.state === "reviewing") {
          previewRef.current = true;
          draftDirty.current = false;
          setReviewPreview(true);
          draftRef.current = e.text ?? "";
          setDraft(draftRef.current);
        }
        if (e.state === "review") {
          previewRef.current = false;
          reviewDoneRef.current = true;
          setReviewPreview(false);
          if (!draftDirty.current) {
            draftRef.current = e.result?.inserted ?? "";
            setDraft(draftRef.current);
          }
        }
        if (["inserted", "empty", "cancelled", "error"].includes(e.state)) {
          previewRef.current = false;
          reviewDoneRef.current = true;
          setReviewPreview(false);
        }
        if (e.error) setError(errorMessage(e.error.code, e.error.detail));
        if (e.result?.inserted) setText(e.result.inserted);
      }),
      // A pinned overlay falls back to its idle card after each dictation.
      on<null>("overlay://idle", () => {
        previewRef.current = false;
        reviewDoneRef.current = false;
        reviewReadyRef.current = false;
        draftRef.current = "";
        draftDirty.current = false;
        setState("idle");
        setText("");
        setError(null);
        setTargetApp(null);
        setCopied(false);
        setReviewPreview(false);
        setBands([]);
        setLevel(0);
      }),
      on<VoiceEvent>("voice://event", (e) => {
        if (e.type === "level") {
          setLevel(e.value);
          setBands(e.bands);
        }
        if (e.type === "partial") setText(e.text);
        if (e.type === "transcript" && e.text) setText(e.text);
      }),
      on<string>("dictation://review-shortcut", (action) => {
        if (action === "cancel") {
          void ipc.dictationCancel().catch(quiet);
          return;
        }
        if (!reviewReadyRef.current) return;
        if (action === "insert") void ipc.dictationConfirm(draftRef.current).catch(quiet);
        if (action === "retry") void ipc.dictationRetry().catch(quiet);
      }),
    ];
    return () => subs.forEach((s) => void s.then((u) => u()));
  }, []);

  useEffect(() => {
    document.documentElement.style.setProperty("--ov-a", `${opacity}%`);
  }, [opacity]);

  useEffect(() => {
    textRef.current?.scrollTo({ top: textRef.current.scrollHeight });
  }, [text]);

  const reviewEditorOpen = reviewPreview || state === "review";
  const reviewReady = state === "review";
  useEffect(() => {
    if (!reviewEditorOpen) return;
    void ipc.shortcutsCapture(true, true).catch((error) => {
      setError(`Review keyboard shortcuts could not be registered: ${String(error)}`);
    });
    return () => {
      void ipc.shortcutsCapture(false).catch(quiet);
    };
  }, [reviewEditorOpen]);

  useEffect(() => {
    if (!reviewEditorOpen) return;
    // The window only just became focusable; give it a beat before typing focus.
    const id = setTimeout(() => {
      const el = editRef.current;
      if (!el) return;
      el.focus();
      el.setSelectionRange(el.value.length, el.value.length);
    }, 60);
    return () => clearTimeout(id);
  }, [reviewEditorOpen]);

  const listening = state === "listening";
  const idle = state === "idle";
  const miniView = mini;
  // Bubble style: a mic circle stays on screen; the pill (without the mic)
  // shows above it while dictating, while hovered or while its menu is open.
  const bubble = miniView && overlayStyle === "bubble";
  const pillShown = !bubble || !idle || hover || langOpen;
  const [winW, setWinW] = useState(window.innerWidth);
  useEffect(() => {
    const onResize = () => setWinW(window.innerWidth);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  // The window region follows the visible shapes: the circle, plus the pill
  // while it shows. The pill's shape is added before it fades in and dropped
  // once it has faded out. Computed from the layout constants (not measured)
  // because the pill is mid-animation when this runs.
  useEffect(() => {
    if (!bubble) {
      void ipc.dictationOverlayCard(null).catch(quiet);
      return;
    }
    const room = parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--menu-room")) || 300;
    const orbTop = room + ORB_TOP;
    const orb: [number, number, number, number] = [winW / 2 - ORB / 2, orbTop, ORB, ORB];
    const pill: [number, number, number, number] = [8, orbTop - ORB_GAP - PILL_H, winW - 16, PILL_H];
    if (pillShown) {
      void ipc.dictationOverlayCard([orb, pill]).catch(quiet);
      return;
    }
    const id = window.setTimeout(() => void ipc.dictationOverlayCard([orb]).catch(quiet), PILL_FADE_MS);
    return () => window.clearTimeout(id);
  }, [bubble, pillShown, winW]);
  // Never leave the window clipped to the bubble when the page goes away.
  useEffect(() => () => void ipc.dictationOverlayCard(null).catch(quiet), []);
  // The pill always fits its header: grow (and save) the width when the
  // current state needs more room than it has.
  const fitSent = useRef(0);
  useLayoutEffect(() => {
    const head = headRef.current;
    if (!miniView || !pillShown || !head) return;
    const need = pillMinWidth(head);
    if (need > window.innerWidth && need !== fitSent.current) {
      fitSent.current = need;
      void ipc.overlayFitWidth(need).catch(quiet);
    }
  });
  const targetTracking = !miniView && (listening
    || state === "transcribing"
    || state === "correcting"
    || reviewEditorOpen);
  // Small layout: what was heard floats above the pill like a subtitle, only
  // while there is something to show. Its rectangle is added to the window's
  // visible region (the same mechanism the language menu uses).
  const subRef = useRef<HTMLDivElement>(null);
  // In review it is the editor instead, so the pill never grows.
  const showSub = miniView && !langOpen && (reviewEditorOpen || (!!text && ["listening", "transcribing", "correcting", "inserted"].includes(state)));
  useEffect(() => {
    const el = subRef.current;
    if (!showSub || !el) return;
    const ta = editRef.current;
    if (reviewEditorOpen && ta) {
      ta.style.height = "auto";
      ta.style.height = `${Math.min(ta.scrollHeight, 110)}px`;
    } else {
      el.scrollTop = el.scrollHeight;
    }
    const report = () => {
      const r = el.getBoundingClientRect();
      void ipc.dictationOverlayBubble([r.left, r.top, r.width, r.height]).catch(quiet);
    };
    report();
    // The pill's width can be dragged: follow it.
    const ro = new ResizeObserver(report);
    ro.observe(el);
    return () => ro.disconnect();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [showSub, reviewEditorOpen, text, draft, state, reviewReady, error]);
  useEffect(() => {
    if (!showSub) return;
    return () => void ipc.dictationOverlayBubble(null).catch(quiet);
  }, [showSub]);
  // Small pill: the free room shows the app the text goes into.
  const miniApp = miniView && targetApp && ["listening", "transcribing", "correcting", "reviewing", "review"].includes(state) ? targetApp : null;
  const micClick = () => void (listening ? ipc.dictationInsertNow(text) : ipc.dictationToggle()).catch(quiet);
  let icon = listening ? (
    <MicBtn live onClick={micClick} />
  ) : (
    <span className="overlay-rec" aria-hidden>
      <Mic size={12} />
    </span>
  );
  let label = t("overlay.listeningLabel");
  switch (state) {
    case "idle":
      icon = <MicBtn live={false} onClick={micClick} />;
      label = t("overlay.idle");
      break;
    case "transcribing":
      icon = <span className="spinner" />;
      label = t("overlay.transcribing");
      break;
    case "correcting":
      icon = <span className="spinner" />;
      label = t("overlay.correcting");
      break;
    case "review":
      icon = <Pencil size={16} style={{ color: "var(--accent)" }} />;
      label = t("overlay.review");
      break;
    case "reviewing":
      icon = <span className="spinner" />;
      label = t("overlay.transcribing");
      break;
    case "inserted":
      icon = <CheckCircle2 size={16} style={{ color: "var(--success)" }} />;
      label = t("overlay.inserted");
      break;
    case "empty":
      icon = <XCircle size={16} style={{ color: "var(--text-faint)" }} />;
      label = t("overlay.empty");
      break;
    case "cancelled":
      icon = <XCircle size={16} style={{ color: "var(--text-faint)" }} />;
      label = t("overlay.cancelled");
      break;
    case "error":
      icon = <XCircle size={16} style={{ color: "var(--danger)" }} />;
      label = error ?? t("overlay.error");
      break;
  }

  const keysHint = reviewBeforeInsert
    ? mode === "toggle" ? t("overlay.hintToggleReview") : t("overlay.hintReview")
    : mode === "toggle" ? t("overlay.hintToggle") : t("overlay.hint");
  const language = useSettings((st) => st.settings?.language || "auto");
  const shown = useSettings((st) => st.settings?.shownLanguages);
  const languageOptions = useMemo(
    () => [
      { value: "auto", short: t("overlay.auto"), name: t("app.automatic") },
      ...KNOWN_LANGUAGES.filter((l) => !shown?.length || shown.includes(l.code) || l.code === language).map((l) => ({ value: l.code, short: l.code.toUpperCase(), name: l.name === l.native ? l.name : `${l.name} · ${l.native}` })),
    ],
    [shown, language]
  );
  const changeLanguage = (code: string) => {
    void ipc
      .dictationSetLanguage(code)
      // Mid-recording the recognizer is already built: restart on the new language.
      .then(() => (listening ? ipc.dictationRetry() : undefined))
      .catch(quiet);
  };
  const confirm = () => {
    if (reviewReady) void ipc.dictationConfirm(draft).catch(quiet);
  };
  const copyDraft = () => {
    void navigator.clipboard
      .writeText(draft)
      .then(() => setCopied(true))
      .catch((e: unknown) => setError(`Could not copy dictation text: ${String(e)}`));
  };

  const headLabel = (
    <span className={`overlay-label${hint ? " hint" : miniApp ? " app" : ""}`}>
      {hint ?? miniApp ?? label}
    </span>
  );
  const langPill = (miniView ? idle || listening || state === "review" : listening || state === "review") && (
    <div className="overlay-pills">
      <PillMenu label={t("overlay.language")} icon={<Globe size={12} aria-hidden />} value={language} options={languageOptions} onChange={changeLanguage} resetKey={state} searchable={languageOptions.length > 4} searchPlaceholder={t("overlay.searchLanguages")} openRef={openLang} onOpenChange={setLangOpen} />
    </div>
  );
  const headButtons = (
    <>
      {showSettingsBtn && !reviewEditorOpen && (
        <HeadBtn label={t("overlay.settings")} onClick={() => void ipc.settingsOpen().catch(quiet)}>
          <SettingsIcon size={13} />
        </HeadBtn>
      )}
      {layouts === "both" && !reviewEditorOpen && (
        <HeadBtn label={t(mini ? "overlay.makeFull" : "overlay.makeMini")} onClick={() => void ipc.overlaySetMini(!mini).catch(quiet)}>
          {mini ? <Maximize2 size={13} /> : <Minimize2 size={13} />}
        </HeadBtn>
      )}
      {!(miniView && !idle) && (
        <HeadBtn label={t(pinned ? "overlay.unpin" : "overlay.pin")} onClick={() => void ipc.overlaySetPinned(!pinned).catch(quiet)}>
          {pinned ? <PinOff size={13} /> : <Pin size={13} />}
        </HeadBtn>
      )}
      {/* Idle: closes the window (and unpins it); never quits the app. */}
      <HeadBtn
        label={idle ? t("overlay.close") : `${t("voice.cancel")} (Esc)`}
        onClick={() => void (idle ? ipc.overlaySetPinned(false) : ipc.dictationCancel()).catch(quiet)}
      >
        <X size={14} />
      </HeadBtn>
    </>
  );

  const editorBody = (
        <>
          <textarea
            ref={editRef}
            className="overlay-edit"
            value={draft}
            dir={draft ? textDir(draft) : "auto"}
            spellCheck
            onChange={(e) => {
              draftDirty.current = true;
              draftRef.current = e.target.value;
              setCopied(false);
              setDraft(e.target.value);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.altKey && (e.ctrlKey || e.metaKey)) {
                e.preventDefault();
                e.stopPropagation();
                if (e.shiftKey) {
                  if (reviewReady) void ipc.dictationRetry().catch(quiet);
                } else {
                  confirm();
                }
              } else if (e.key === "Escape") {
                e.preventDefault();
                e.stopPropagation();
                void ipc.dictationCancel().catch(quiet);
              }
            }}
          />
          <div className="overlay-hint overlay-review-keys">
            {error ?? (!reviewReady ? t("overlay.reviewPending") : t("overlay.reviewKeys"))}
          </div>
          <div className="overlay-actions">
            <span style={{ flex: 1 }} />
            <div className="overlay-btn-group" role="group">
              <button type="button" className="btn btn-sm btn-ghost" title={t("overlay.copyHint")} disabled={!reviewReady || !draft} onClick={copyDraft}>
                <Copy size={13} /> {copied ? t("app.copied") : t("app.copy")}
              </button>
              <button type="button" className="btn btn-sm btn-ghost" title={t("overlay.retryHint")} disabled={!reviewReady} onClick={() => void ipc.dictationRetry().catch(quiet)}>
                <RotateCcw size={13} /> {t("overlay.retry")}
              </button>
              <button type="button" className="btn btn-sm btn-ghost" title={`${t("voice.cancel")} (Esc)`} onClick={() => void ipc.dictationCancel().catch(quiet)}>
                <X size={13} /> {t("voice.cancel")}
              </button>
            </div>
            <button type="button" className="btn btn-sm btn-primary" title={t("overlay.insertHint")} disabled={!reviewReady} onClick={confirm}>
              <Check size={13} /> {t("overlay.insert")}
            </button>
          </div>
        </>
  );

  const resizeEdge = miniView && !sizeLocked && <ResizeEdge minWidth={() => (headRef.current ? pillMinWidth(headRef.current) : 0)} />;
  const sub =
    showSub &&
    (reviewEditorOpen ? (
      <div className="overlay-sub editing" ref={subRef}>
        {editorBody}
      </div>
    ) : (
      <div className="overlay-sub" ref={subRef} dir={textDir(text)}>
        {text}
      </div>
    ));
  // Drag to move; double-click puts the overlay back in its default spot.
  const dragProps = {
    onPointerDown: drag.onPointerDown,
    onPointerMove: drag.onPointerMove,
    onPointerUp: drag.onPointerUp,
    onDoubleClick: () => (positionLocked ? undefined : void ipc.dictationResetOverlayPosition().catch(quiet)),
  };
  // Header: what is happening, the language, and close. In the bubble style
  // the mic lives in the circle below, not here.
  const head = (
    <div
      ref={headRef}
      className={`overlay-head${bubble ? " split" : ""}${positionLocked ? " locked" : ""}`}
      {...dragProps}
      onContextMenu={(e) => {
        // Right click on the pill: the language menu.
        e.preventDefault();
        openLang.current?.();
      }}
    >
      {bubble ? (
        // Bubble pill: the status sits in the middle, over the circle; the
        // language chip holds the left edge and the buttons the right edge.
        <>
          <div className="overlay-side start">{langPill}</div>
          <div className="overlay-center">
            {listening && <VoiceBars bands={bands} level={level} label={t("voice.level")} />}
            {headLabel}
          </div>
          <div className="overlay-side end">{headButtons}</div>
        </>
      ) : (
        <>
          <span className="overlay-icon">{icon}</span>
          {miniView && listening && <VoiceBars bands={bands} level={level} label={t("voice.level")} />}
          {headLabel}
          {!miniView && listening && <VoiceBars bands={bands} level={level} label={t("voice.level")} />}
          {!miniView && <span style={{ flex: 1 }} />}
          {langPill}
          {headButtons}
        </>
      )}
    </div>
  );

  return (
    <div
      className={`overlay-card${state === "inserted" ? " done" : ""}${miniView ? " mini" : ""}${bubble ? " bubble" : ""}`}
      role="status"
      aria-live="polite"
      onPointerEnter={hoverIn}
      onPointerLeave={hoverOut}
    >
      {targetTracking && (
        <div
          className="overlay-app-tab"
          title={targetApp ? t("overlay.targetWindow", { app: targetApp }) : t("overlay.targetUnknown")}
          aria-label={targetApp ? t("overlay.targetWindow", { app: targetApp }) : t("overlay.targetUnknown")}
        >
          <span className="overlay-app-tab-label">
            {targetApp ? (
              <>
                <span className="overlay-app-tab-prefix">{t("overlay.targetPrefix")}</span>{" "}
                <span className="overlay-app-tab-app">{targetApp}</span>
              </>
            ) : (
              <span className="overlay-app-tab-unknown">{t("overlay.targetUnknown")}</span>
            )}
          </span>
        </div>
      )}
      <TooltipLayer inline={setHint} />
      {bubble ? (
        <>
          {/* The circle: state icon and mic button; drags the overlay. A
              right click shows the pill. */}
          <div
            className={`overlay-orb${positionLocked ? " locked" : ""}`}
            {...dragProps}
            onContextMenu={(e) => {
              e.preventDefault();
              setHover(true);
            }}
          >
            {icon}
          </div>
          <div className={`overlay-pill${pillShown ? " shown" : ""}`} aria-hidden={!pillShown}>
            {resizeEdge}
            {sub}
            {/* The drawn pill; it pops out of the circle. */}
            <div className="overlay-pill-body">{head}</div>
          </div>
        </>
      ) : (
        <>
          {resizeEdge}
          {sub}
          {head}
        </>
      )}
      {reviewEditorOpen && !miniView ? (
        editorBody      ) : miniView ? null : (
        <>
          <div className={`overlay-text${text ? "" : " empty"}`} ref={textRef} dir={text ? textDir(text) : "auto"}>
            {text || (idle ? t("overlay.idleHint", { keys: prettyAccelerator(shortcut) }) : t("overlay.speakNow"))}
          </div>
          {listening && (
            <div className="overlay-actions">
              <span className="overlay-hint">{keysHint}</span>
              <button type="button" className="btn btn-sm btn-ghost" title={`${t("voice.cancel")} (Esc)`} onClick={() => void ipc.dictationCancel().catch(quiet)}>
                <X size={13} /> {t("voice.cancel")}
              </button>
              <button type="button" className="btn btn-sm btn-ghost" title={t("overlay.retryHint")} onClick={() => void ipc.dictationRetry().catch(quiet)}>
                <RotateCcw size={13} /> {t("overlay.restart")}
              </button>
              <button
                type="button"
                className="btn btn-sm btn-primary"
                title={t(reviewBeforeInsert ? "overlay.reviewNowHint" : "overlay.insertNowHint")}
                onClick={() => void ipc.dictationInsertNow(text).catch(quiet)}
              >
                {reviewBeforeInsert ? <Pencil size={13} /> : <Check size={13} />}
                {t(reviewBeforeInsert ? "overlay.reviewNow" : "overlay.insert")}
              </button>
            </div>
          )}
        </>
      )}
    </div>
  );
}
