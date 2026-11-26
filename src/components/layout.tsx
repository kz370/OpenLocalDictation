import type { ReactNode } from "react";

export function Card(props: { title?: string; icon?: ReactNode; actions?: ReactNode; children: ReactNode }) {
  return (
    <section className="card">
      {(props.title || props.actions) && (
        <div className="card-head">
          {props.title && (
            <h2>
              {props.icon && <span className="card-icon" aria-hidden>{props.icon}</span>}
              {props.title}
            </h2>
          )}
          {props.actions && <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>{props.actions}</div>}
        </div>
      )}
      <div className="card-body">{props.children}</div>
    </section>
  );
}

export function Row(props: { label: string; icon?: ReactNode; hint?: string; htmlFor?: string; children: ReactNode; end?: boolean }) {
  return (
    <div className="row">
      <label className={`row-label${props.icon ? " has-icon" : ""}`} htmlFor={props.htmlFor}>
        {props.icon && <span className="row-icon" aria-hidden>{props.icon}</span>}
        <span className="row-label-text">
          {props.label}
          {props.hint && <span className="row-hint">{props.hint}</span>}
        </span>
      </label>
      <div className={`row-control${props.end ? " end" : ""}`}>{props.children}</div>
    </div>
  );
}
