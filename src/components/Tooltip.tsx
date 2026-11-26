import { useEffect, useLayoutEffect, useRef, useState } from "react";

type Rect = [number, number, number, number];

interface Props {
  /** Tooltips open above the element, or below it when `prefer` is "below" (or when there is no room above). */
  prefer?: "above" | "below";
  /** Called with the tooltip's rectangle (CSS px) before it is shown, and with null when it goes away.
   *  The overlay window uses it to add the tooltip to its visible region. */
  onRect?: (rect: Rect | null) => Promise<unknown> | void;
}

const SHOW_DELAY_MS = 450;
const GAP = 8;
const EDGE = 4;

/**
 * Themed tooltips for the whole page. Every element with a `title` attribute
 * gets one: the native tooltip (which ignores the app theme) is suppressed by
 * moving the text into `data-tip` while the pointer is over it, and put back
 * afterwards. Mount once per window.
 */
export function TooltipLayer({ prefer = "above", onRect }: Props) {
  const [tip, setTip] = useState<{ text: string; anchor: DOMRect } | null>(null);
  const [pos, setPos] = useState<{ x: number; y: number } | null>(null);
  const ref = useRef<HTMLDivElement>(null);
  const onRectRef = useRef(onRect);
  onRectRef.current = onRect;
  const shown = useRef(false);

  useEffect(() => {
    let timer: number | undefined;
    let current: HTMLElement | null = null;

    const hide = () => {
      window.clearTimeout(timer);
      if (current) {
        const text = current.dataset.tip;
        if (text !== undefined && !current.hasAttribute("title")) current.setAttribute("title", text);
        delete current.dataset.tip;
        current = null;
      }
      if (shown.current) {
        shown.current = false;
        void onRectRef.current?.(null);
      }
      setTip(null);
      setPos(null);
    };

    const over = (e: MouseEvent) => {
      const el = (e.target as HTMLElement | null)?.closest?.("[title],[data-tip]") as HTMLElement | null;
      if (el === current) return;
      hide();
      if (!el) return;
      const text = el.getAttribute("title") ?? el.dataset.tip;
      if (!text) return;
      el.dataset.tip = text;
      el.removeAttribute("title");
      current = el;
      timer = window.setTimeout(() => setTip({ text, anchor: el.getBoundingClientRect() }), SHOW_DELAY_MS);
    };

    document.addEventListener("mouseover", over);
    document.addEventListener("pointerdown", hide, true);
    document.addEventListener("keydown", hide, true);
    window.addEventListener("blur", hide);
    document.documentElement.addEventListener("mouseleave", hide);
    return () => {
      document.removeEventListener("mouseover", over);
      document.removeEventListener("pointerdown", hide, true);
      document.removeEventListener("keydown", hide, true);
      window.removeEventListener("blur", hide);
      document.documentElement.removeEventListener("mouseleave", hide);
      hide();
    };
  }, []);

  // Measure the hidden tooltip, place it, tell the window about it, then show it.
  useLayoutEffect(() => {
    const el = ref.current;
    if (!tip || !el) return;
    const { width: w, height: h } = el.getBoundingClientRect();
    const a = tip.anchor;
    const above = a.top - GAP - h;
    const below = a.bottom + GAP;
    const fitsBelow = below + h <= window.innerHeight - EDGE;
    const y = prefer === "below" ? (fitsBelow || above < EDGE ? below : above) : above >= EDGE ? above : below;
    const x = Math.min(Math.max(a.left + a.width / 2 - w / 2, EDGE), Math.max(EDGE, window.innerWidth - w - EDGE));
    let cancelled = false;
    shown.current = true;
    void Promise.resolve(onRectRef.current?.([x, y, w, h])).then(() => {
      if (!cancelled) setPos({ x, y });
    });
    return () => {
      cancelled = true;
    };
  }, [tip, prefer]);

  if (!tip) return null;
  return (
    <div
      ref={ref}
      className="tooltip"
      role="tooltip"
      style={{ left: pos?.x ?? 0, top: pos?.y ?? 0, visibility: pos ? "visible" : "hidden" }}
    >
      {tip.text}
    </div>
  );
}
