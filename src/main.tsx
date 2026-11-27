import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { useSettings } from "./settingsStore";
import { loadProfiles } from "./profiles";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { Overlay } from "./pages/Overlay";
import { TooltipLayer } from "./components/Tooltip";
import { SettingsPage } from "./pages/Settings";
import "./styles/tokens.css";
import "./styles/base.css";
import "./styles/app.css";
import "./styles/settings.css";

type Route = "settings" | "overlay";

const routeFromHash = (): Route => (location.hash.startsWith("#/overlay") ? "overlay" : "settings");

// Disable the webview's default context menu outside editable fields.
document.addEventListener("contextmenu", (e) => {
  if (!(e.target as HTMLElement).closest("input, textarea")) e.preventDefault();
});

function Root() {
  const [route] = useState(routeFromHash);
  const settings = useSettings((s) => s.settings);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    document.documentElement.dataset.route = route;
    let cancelled = false;
    let un: (() => void) | undefined;
    (async () => {
      await loadProfiles();
      await useSettings.getState().load();
      const u = await useSettings.getState().subscribe();
      if (cancelled) u();
      else un = u;
    })().catch((e: unknown) => setError(e instanceof Error ? e.message : JSON.stringify(e)));
    return () => {
      cancelled = true;
      un?.();
    };
  }, [route]);

  if (error) {
    return (
      <div style={{ padding: 20, userSelect: "text" }}>
        <h1 style={{ fontSize: 16, margin: "0 0 6px" }}>Open Local Dictation could not start.</h1>
        <p style={{ fontSize: 13, color: "var(--text-muted)" }}>{error}</p>
        <button className="btn" onClick={() => location.reload()}>
          Retry
        </button>
      </div>
    );
  }
  if (!settings) return <div className="boot" aria-busy="true" />;
  return (
    <ErrorBoundary where={route}>
      {route === "overlay" ? <Overlay /> : <SettingsPage />}
      {route === "settings" && <TooltipLayer />}
    </ErrorBoundary>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
