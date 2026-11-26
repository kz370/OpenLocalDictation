import { useEffect, useRef, type ReactNode } from "react";
import { errorMessage } from "../strings";
import type { AppErrorPayload } from "../types";

export function Switch(props: { checked: boolean; onChange: (v: boolean) => void; label: string; disabled?: boolean; id?: string }) {
  return (
    <button id={props.id} type="button" role="switch" className="switch" aria-checked={props.checked} aria-label={props.label} disabled={props.disabled} onClick={() => props.onChange(!props.checked)} />
  );
}

export function Segmented<T extends string>(props: { value: T; options: { value: T; label: string; icon?: ReactNode; disabled?: boolean }[]; onChange: (v: T) => void; label: string }) {
  return (
    <div className="segmented" role="radiogroup" aria-label={props.label}>
      {props.options.map((o) => (
        <button key={o.value} type="button" role="radio" aria-checked={props.value === o.value} disabled={o.disabled} onClick={() => props.onChange(o.value)}>
          {o.icon}
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Dialog(props: { title: string; children: ReactNode; actions: ReactNode; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    ref.current?.querySelector<HTMLElement>("button, input, select, textarea")?.focus();
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && props.onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  return (
    <div className="dialog-backdrop" onMouseDown={(e) => e.target === e.currentTarget && props.onClose()}>
      <div className="dialog" role="dialog" aria-modal="true" aria-label={props.title} ref={ref}>
        <h2>{props.title}</h2>
        {props.children}
        <div className="dialog-actions">{props.actions}</div>
      </div>
    </div>
  );
}

export function ErrorNotice(props: { error: AppErrorPayload; actions?: ReactNode }) {
  return (
    <div className="notice err" role="alert">
      <div style={{ flex: 1 }}>
        <div>{errorMessage(props.error.code)}</div>
        {props.error.detail && (
          <details className="tech">
            <summary>Technical details</summary>
            <pre>{props.error.detail}</pre>
          </details>
        )}
        {props.actions && <div style={{ display: "flex", gap: 6, marginTop: 8 }}>{props.actions}</div>}
      </div>
    </div>
  );
}

/** Detects the base direction of text for correct Arabic/English rendering. */
export function textDir(text: string): "rtl" | "ltr" | "auto" {
  const firstStrong = text.match(/[֐-ࣿיִ-﷿ﹰ-﻿]|[A-Za-zÀ-ɏ]/);
  if (!firstStrong) return "auto";
  return /[A-Za-zÀ-ɏ]/.test(firstStrong[0]) ? "ltr" : "rtl";
}
