import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { Brain, Check, Copy, Home, Languages, Mic, Minus, PictureInPicture2, SlidersHorizontal, Square, X } from "lucide-react";
import { ipc, on } from "../ipc";
import { useSettings } from "../settingsStore";
import { useAppIcon } from "../profiles";
import type { AppInfo, AudioDevice, GpuStatus, ModelEntry, Settings, TaskbarStatus } from "../types";
import { ErrorNotice } from "../components/controls";
import { prettyAccelerator } from "../components/ShortcutInput";
import { Sidebar, type NavItem, type SearchEntry } from "./settings/Sidebar";
import { HomePage } from "./settings/Home";
import { ModelsPage } from "./settings/Models";
import { LanguagesPage } from "./settings/Languages";
import { DictationPage } from "./settings/Dictation";
import { OverlayPage } from "./settings/OverlaySettings";
import { AppPage } from "./settings/AppSettings";

type Section = "home" | "models" | "languages" | "dictation" | "overlay" | "app";

const SECTIONS: { id: Section; label: string; title: string; desc: string; icon: ReactNode }[] = [
  { id: "home", label: "Home", title: "Home", desc: "Is everything ready? Check the essentials at a glance.", icon: <Home size={16} /> },
  { id: "models", label: "Models", title: "Models", desc: "Choose and download the speech model, and whether it runs on the CPU or the GPU.", icon: <Brain size={16} /> },
  { id: "languages", label: "Languages", title: "Languages", desc: "Pick the language you speak, and a bigger model for the hard ones.", icon: <Languages size={16} /> },
  { id: "dictation", label: "Dictation", title: "Dictation", desc: "The shortcut, the microphone, and how the text is inserted.", icon: <Mic size={16} /> },
  { id: "overlay", label: "Overlay", title: "Overlay", desc: "How the small dictation window looks and where it sits.", icon: <PictureInPicture2 size={16} /> },
  { id: "app", label: "App", title: "App", desc: "Start-up, theme, accent colour and logs.", icon: <SlidersHorizontal size={16} /> },
];

/** Labels of the rows, for the sidebar search (they match the rows' `data-row`). */
const INDEX: SearchEntry<Section>[] = [
  { section: "models", label: "Speech model", keywords: "whisper download load unload auto-load hide" },
  { section: "models", label: "Models folder", keywords: "folder directory path own model add moonshine" },
  { section: "models", label: "Run speech recognition on", keywords: "cpu gpu cuda nvidia device processing" },
  { section: "models", label: "CUDA libraries", keywords: "gpu nvidia download" },
  { section: "languages", label: "Language", keywords: "dictation language auto detect arabic" },
  { section: "languages", label: "Languages in the overlay menu", keywords: "filter shown list" },
  { section: "languages", label: "Model per language", keywords: "bigger model hard languages hindi thai" },
  { section: "dictation", label: "Shortcut", keywords: "hotkey keyboard key" },
  { section: "dictation", label: "Shortcut behavior", keywords: "hold toggle press talk" },
  { section: "dictation", label: "Microphone", keywords: "mic input device test level" },
  { section: "dictation", label: "Review before inserting", keywords: "edit confirm ctrl enter live typing" },
  { section: "dictation", label: "Insert by", keywords: "typing pasting clipboard" },
  { section: "dictation", label: "Add a space after the text", keywords: "trailing space" },
  { section: "overlay", label: "Overlay size", keywords: "small large pill" },
  { section: "overlay", label: "Small overlay style", keywords: "pill bubble" },
  { section: "overlay", label: "Overlay opacity", keywords: "transparent see-through" },
  { section: "overlay", label: "Settings button", keywords: "gear" },
  { section: "overlay", label: "Mic button in the taskbar", keywords: "taskbar widget" },
  { section: "overlay", label: "Pin the overlay", keywords: "always on top" },
  { section: "overlay", label: "Move sideways only", keywords: "horizontal" },
  { section: "overlay", label: "Lock the overlay position", keywords: "drag" },
  { section: "overlay", label: "Lock the small overlay's size", keywords: "resize width" },
  { section: "overlay", label: "Reset positions", keywords: "reset default overlay taskbar button" },
  { section: "app", label: "Start with Windows", keywords: "autostart startup boot login" },
  { section: "app", label: "Theme", keywords: "dark light system colour" },
  { section: "app", label: "Accent colour", keywords: "color profile theme tray icon taskbar green teal blue amber slate" },
  { section: "app", label: "Logs", keywords: "version open folder" },
];

const STORE_KEY = "settings-section";
const initialSection = (): Section => {
  const saved = sessionStorage.getItem(STORE_KEY);
  return SECTIONS.some((s) => s.id === saved) ? (saved as Section) : "home";
};

export function SettingsPage() {
  const appIcon = useAppIcon();
  const [section, setSection] = useState<Section>(initialSection);
  const settings = useSettings((s) => s.settings);
  const update = useSettings((s) => s.update);
  const saveError = useSettings((s) => s.error);
  const savedAt = useSettings((s) => s.savedAt);
  const [mics, setMics] = useState<AudioDevice[]>([]);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [models, setModels] = useState<ModelEntry[] | null>(null);
  const [gpu, setGpu] = useState<GpuStatus | null>(null);
  const [shortcutErrors, setShortcutErrors] = useState<string[]>([]);
  const [taskbar, setTaskbar] = useState<TaskbarStatus>({ shown: false, reason: null });
  const [savedShown, setSavedShown] = useState(false);

  const go = useCallback((s: Section) => {
    sessionStorage.setItem(STORE_KEY, s);
    setSection(s);
  }, []);

  /** Opens a section and flashes the row with this label. */
  const jump = useCallback(
    (s: Section, label: string) => {
      go(s);
      window.setTimeout(() => {
        const row = [...document.querySelectorAll<HTMLElement>("[data-row]")].find((r) => r.dataset.row === label);
        if (!row) return;
        row.scrollIntoView({ block: "center", behavior: "smooth" });
        row.classList.remove("hit");
        void row.offsetWidth; // restart the animation
        row.classList.add("hit");
        window.setTimeout(() => row.classList.remove("hit"), 2300);
      }, 80);
    },
    [go],
  );

  useEffect(() => {
    void ipc.audioInputs().then(setMics).catch(() => undefined);
    void ipc.appInfo().then(setInfo);
  }, []);
  useEffect(() => {
    void ipc.shortcutErrors().then(setShortcutErrors);
  }, [settings?.shortcut]);
  // Whether the taskbar button found its place (it can take a moment after the switch).
  useEffect(() => {
    void ipc.taskbarWidgetStatus().then(setTaskbar).catch(() => undefined);
    const sub = on<TaskbarStatus>("taskbar://status", setTaskbar);
    return () => void sub.then((u) => u());
  }, []);
  // The title bar's maximize button: Maximize, or Restore while maximized (also by double-clicking the bar).
  const [maximized, setMaximized] = useState(false);
  useEffect(() => {
    void ipc.settingsIsMaximized().then(setMaximized).catch(() => undefined);
    const sub = on<boolean>("settings://maximized", setMaximized);
    return () => void sub.then((u) => u());
  }, []);
  // What the sidebar flags: no model yet, or a GPU change waiting for a restart.
  useEffect(() => {
    const refreshModels = () => void ipc.modelsList().then(setModels).catch(() => undefined);
    const refreshGpu = () => void ipc.gpuStatus().then(setGpu).catch(() => undefined);
    refreshModels();
    refreshGpu();
    const subs = [on("models://changed", refreshModels), on("gpu://changed", refreshGpu)];
    return () => subs.forEach((s) => void s.then((u) => u()));
  }, []);
  // "Saved" chip after every change.
  useEffect(() => {
    if (!savedAt) return;
    setSavedShown(true);
    const t = window.setTimeout(() => setSavedShown(false), 1600);
    return () => window.clearTimeout(t);
  }, [savedAt]);
  // Ctrl+1 … Ctrl+6 open the sections.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.ctrlKey || e.altKey || e.shiftKey) return;
      const n = Number(e.key);
      if (n >= 1 && n <= SECTIONS.length) {
        e.preventDefault();
        go(SECTIONS[n - 1].id);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [go]);

  const items: NavItem<Section>[] = useMemo(() => {
    const attention: Partial<Record<Section, string>> = {};
    if (models !== null && !models.some((m) => m.installed)) attention.models = "no speech model yet";
    else if (gpu && (gpu.restartRequired || (gpu.active && settings?.device !== "gpu"))) attention.models = "restart needed";
    if (shortcutErrors.length > 0) attention.dictation = "the shortcut could not be registered";
    if (models !== null && !models.some((m) => m.installed)) attention.home = "no speech model yet";
    return SECTIONS.map((s) => ({ id: s.id, label: s.label, icon: s.icon, attention: attention[s.id] }));
  }, [models, gpu, shortcutErrors, settings?.device]);

  if (!settings) return null;
  const set = (m: (d: Settings) => void) => void update(m);
  const current = SECTIONS.find((s) => s.id === section) ?? SECTIONS[0];
  const status = `Version ${info?.version ?? "…"} · ${gpu?.active ? "GPU" : "CPU"}`;

  return (
    <div className="settings-shell">
      <header className="titlebar" data-tauri-drag-region>
        <span className="titlebar-title" data-tauri-drag-region>
          <img className="app-icon-sm" src={appIcon} alt="" aria-hidden /> Open Local Dictation
        </span>
        <div className="titlebar-buttons">
          <button type="button" className="icon-btn" aria-label="Minimize" title="Minimize" onClick={() => void ipc.settingsMinimize()}>
            <Minus size={14} />
          </button>
          <button type="button" className="icon-btn" aria-label={maximized ? "Restore" : "Maximize"} title={maximized ? "Restore" : "Maximize"} onClick={() => void ipc.settingsToggleMaximize().then(setMaximized).catch(() => undefined)}>
            {maximized ? <Copy size={12} /> : <Square size={12} />}
          </button>
          <button type="button" className="icon-btn close" aria-label="Close" title="Close (keeps running in the tray)" onClick={() => void ipc.settingsClose()}>
            <X size={14} />
          </button>
        </div>
      </header>
      <div className="settings-body">
        <Sidebar
          items={items}
          active={section}
          onSelect={go}
          index={INDEX}
          onJump={jump}
          sectionLabel={(id) => SECTIONS.find((s) => s.id === id)?.label ?? id}
          shortcut={prettyAccelerator(settings.shortcut)}
          status={status}
        />
        <main className="content">
          <div className="content-head">
            <div>
              <h1>{current.title}</h1>
              <p className="page-desc">{current.desc}</p>
            </div>
            {savedShown && (
              <span className="saved-chip" role="status">
                <Check size={12} /> Saved
              </span>
            )}
          </div>
          {saveError && <ErrorNotice error={saveError} />}
          {shortcutErrors.length > 0 && (
            <div className="notice warn">
              <div>
                The shortcut could not be registered — another app may be using it. Pick a different one on the Dictation page.
                <ul style={{ margin: "4px 0 0", paddingInlineStart: 18 }}>
                  {shortcutErrors.map((e) => (
                    <li key={e}>{e}</li>
                  ))}
                </ul>
              </div>
            </div>
          )}
          <div className="page" key={section}>
            {section === "home" && <HomePage settings={settings} info={info} gpu={gpu} mics={mics} go={go} />}
            {section === "models" && <ModelsPage settings={settings} update={set} />}
            {section === "languages" && <LanguagesPage settings={settings} update={set} />}
            {section === "dictation" && <DictationPage settings={settings} update={set} mics={mics} />}
            {section === "overlay" && <OverlayPage settings={settings} update={set} taskbar={taskbar} />}
            {section === "app" && <AppPage settings={settings} update={set} info={info} />}
          </div>
        </main>
      </div>
    </div>
  );
}
