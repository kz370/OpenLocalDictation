import { useEffect, useLayoutEffect, useRef, useState } from "react";

interface Props {
  /** Inline mode (the overlay): no bubble is drawn. The hovered element's tooltip text is handed to this
   *  callback instead (and null when the pointer leaves), so the page can show it inside itself. */
  inline?: (text: string | null) => void;
}

const SHOW_DELAY_MS = 450;
const INLINE_DELAY_MS = 250;
const GAP = 8;
const EDGE = 4;

/**
 * Themed tooltips for the whole page. Every element with a `title` attribute
 * gets one: the native tooltip (which ignores the app theme) is suppressed by
 * moving the text into `data-tip` while the pointer is over it, and put back
 * afterwards. Mount once per window.
 */
export function TooltipLayer({ inline }: Props) {
  const [tip, setTip] = useState<{ text: string; anchor: DOMRect } | null>(null);
  const [pos, setPos] = useState<{ x: number; y: number } | null>(null);
  const ref = useRef<HTMLDivElement>(null);
  const inlineRef = useRef(inline);
  inlineRef.current = inline;
  const inlineShown = useRef(false);

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
      if (inlineShown.current) {
        inlineShown.current = false;
        inlineRef.current?.(null);
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
      if (inlineRef.current) {
        timer = window.setTimeout(() => {
          inlineShown.current = true;
          inlineRef.current?.(text);
        }, INLINE_DELAY_MS);
      } else {
        timer = window.setTimeout(() => setTip({ text, anchor: el.getBoundingClientRect() }), SHOW_DELAY_MS);
      }
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

  // Measure the hidden tooltip, place it above (or below when there is no room), then show it.
  useLayoutEffect(() => {
    const el = ref.current;
    if (!tip || !el) return;
    const { width: w, height: h } = el.getBoundingClientRect();
    const a = tip.anchor;
    const above = a.top - GAP - h;
    const y = above >= EDGE ? above : a.bottom + GAP;
    const x = Math.min(Math.max(a.left + a.width / 2 - w / 2, EDGE), Math.max(EDGE, window.innerWidth - w - EDGE));
    setPos({ x, y });
  }, [tip]);

  if (!tip || inline) return null;
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
