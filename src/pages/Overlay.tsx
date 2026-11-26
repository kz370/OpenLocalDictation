import { useEffect, useMemo, useRef, useState } from "react";
import { Check, CheckCircle2, ChevronDown, Copy, Globe, Maximize2, Mic, Minimize2, Pencil, Pin, PinOff, RotateCcw, Square, X, XCircle } from "lucide-react";
import { getCurrentWindow, PhysicalPosition } from "@tauri-apps/api/window";
import { ipc, on } from "../ipc";
import { KNOWN_LANGUAGES } from "../knownLanguages";
import { useSettings } from "../settingsStore";
import { errorMessage, t } from "../strings";
import type { DictationStateEvent, DictationTargetEvent, VoiceEvent } from "../types";
import { textDir } from "../components/controls";
import { VoiceBars } from "../components/VoiceBars";

/** Free drag on the overlay's header strip; Shift locks to horizontal-only,
 * Alt locks to vertical-only. Uses manual `setPosition` (not Tauri's
 * `startDragging`, which hands off to the OS and can't be axis-constrained
 * mid-drag), throttled to one IPC call per frame. */
function useOverlayDrag() {
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
      if (e.button !== 0) return;
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
      if (e.shiftKey) dy = 0;
      if (e.altKey) dx = 0;
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

/** Drag handle on the small pill's right edge: width only. The window follows
 *  live; the width is saved when the drag ends. Double-click restores the
 *  default width. */
function ResizeEdge() {
  const drag = useRef<{ x: number; w: number; frame: number | null; next: number } | null>(null);
  const send = (w: number, commit: boolean) => void ipc.overlayResize(w, commit).catch(quiet);
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
        d.next = Math.min(MINI_MAX_W, Math.max(MINI_MIN_W, Math.round(d.w + e.screenX - d.x)));
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

const MENU_ROW = 27; // option height + gap
const MENU_PAD = 10; // padding + border
const MENU_MAX_ROWS = 6;
const MENU_GAP = 6; // between the pill and the menu
const MENU_W = 210; // matches .overlay-lang-menu
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
}) {
  const [open, setOpen] = useState<{ top: number; right: number } | null>(null);
  const [query, setQuery] = useState("");
  const ref = useRef<HTMLDivElement>(null);
  const btnRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const openRef = useRef(false);
  const owner = useRef({});

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
    const right = document.documentElement.clientWidth - r.right;
    // Unclip the menu's area first, so its first frame isn't cut off.
    menuOwner = owner.current;
    await ipc.dictationOverlayMenu([r.right - MENU_W, top, MENU_W, menuH]).catch(quiet);
    if (!openRef.current) return;
    setOpen({ top, right });
  };

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
        <div ref={menuRef} className="overlay-lang-menu" role="listbox" aria-label={props.label} style={{ top: open.top, right: open.right, maxHeight: menuH }}>
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
  const mode = useSettings((s) => s.settings?.mode ?? "hold");
  const reviewBeforeInsert = useSettings((s) => s.settings?.reviewBeforeInsert ?? true);
  const pinned = useSettings((s) => s.settings?.overlayPinned ?? false);
  const mini = useSettings((s) => s.settings?.overlayMini ?? false);
  const sizeLocked = useSettings((s) => s.settings?.overlaySizeLocked ?? false);
  const shortcut = useSettings((s) => s.settings?.shortcut ?? "");
  const textRef = useRef<HTMLDivElement>(null);
  const editRef = useRef<HTMLTextAreaElement>(null);
  const previewRef = useRef(false);
  const reviewDoneRef = useRef(false);
  const reviewReadyRef = useRef(false);
  const draftRef = useRef("");
  const draftDirty = useRef(false);
  const drag = useOverlayDrag();

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
  const targetTracking = !miniView && (listening
    || state === "transcribing"
    || state === "correcting"
    || reviewEditorOpen);
  // Small layout: what was heard floats above the pill like a subtitle, only
  // while there is something to show. Its rectangle is added to the window's
  // visible region (the same mechanism the language menu uses).
  const subRef = useRef<HTMLDivElement>(null);
  // In review it is the editor instead, so the pill never grows.
  const showSub = miniView && (reviewEditorOpen || (!!text && ["listening", "transcribing", "correcting", "inserted"].includes(state)));
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
      void ipc.dictationOverlayMenu([r.left, r.top, r.width, r.height]).catch(quiet);
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
    return () => void ipc.dictationOverlayMenu(null).catch(quiet);
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

  return (
    <div className={`overlay-card${state === "inserted" ? " done" : ""}${miniView ? " mini" : ""}`} role="status" aria-live="polite">
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
      {miniView && !sizeLocked && <ResizeEdge />}
      {showSub &&
        (reviewEditorOpen ? (
          <div className="overlay-sub editing" ref={subRef}>
            {editorBody}
          </div>
        ) : (
          <div className="overlay-sub" ref={subRef} dir={textDir(text)}>
            {text}
          </div>
        ))}
      {/* Header: what is happening, the language, and close. Drag to move;
          double-click puts the overlay back in its default spot. */}
      <div
        className="overlay-head"
        title={t("overlay.dragHint")}
        onPointerDown={drag.onPointerDown}
        onPointerMove={drag.onPointerMove}
        onPointerUp={drag.onPointerUp}
        onDoubleClick={() => void ipc.dictationResetOverlayPosition().catch(quiet)}
      >
        <span className="overlay-icon">{icon}</span>
        {miniView && listening && <VoiceBars bands={bands} level={level} label={t("voice.level")} />}
        <span className={`overlay-label${miniApp ? " app" : ""}`} title={miniApp ? t("overlay.targetWindow", { app: miniApp }) : undefined}>
          {miniApp ?? label}
        </span>
        {!miniView && listening && <VoiceBars bands={bands} level={level} label={t("voice.level")} />}
        {!miniView && <span style={{ flex: 1 }} />}
        {!miniView && (listening || state === "review") && (
          <div className="overlay-pills">
            <PillMenu label={t("overlay.language")} icon={<Globe size={12} aria-hidden />} value={language} options={languageOptions} onChange={changeLanguage} resetKey={state} searchable searchPlaceholder={t("overlay.searchLanguages")} />
          </div>
        )}
        {!reviewEditorOpen && (
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
      </div>
      {reviewEditorOpen && !miniView ? (
        editorBody      ) : miniView ? null : (
        <>
          <div className={`overlay-text${text ? "" : " empty"}`} ref={textRef} dir={text ? textDir(text) : "auto"}>
            {text || (idle ? t("overlay.idleHint", { keys: shortcut.replace("CommandOrControl", "Ctrl") }) : t("overlay.speakNow"))}
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
