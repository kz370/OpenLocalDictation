import { Fragment, useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { open as pickFolder } from "@tauri-apps/plugin-dialog";
import {
  Brain,
  Check,
  ClipboardPaste,
  Cpu,
  Download,
  Droplets,
  FileText,
  FolderOpen,
  Keyboard,
  Languages,
  ListFilter,
  Lock,
  LockKeyhole,
  Maximize2,
  Mic,
  Minus,
  Monitor,
  Moon,
  MousePointerClick,
  Move,
  MoveHorizontal,
  Palette,
  Pencil,
  PenLine,
  PictureInPicture2,
  Pin,
  Power,
  RotateCcw,
  RotateCw,
  Settings as SettingsIcon,
  SlidersHorizontal,
  Space,
  Sun,
  Type,
  X,
  Zap,
} from "lucide-react";
import { ipc, on, toAppError } from "../ipc";
import { KNOWN_LANGUAGES } from "../knownLanguages";
import { useSettings } from "../settingsStore";
import { formatBytes } from "../strings";
import type { AppErrorPayload, AppInfo, AudioDevice, DownloadProgress, GpuProgress, GpuStatus, ModelEntry, Settings } from "../types";
import { Dialog, ErrorNotice, Segmented, Switch } from "../components/controls";
import { Card, Row } from "../components/layout";
import { SearchSelect, type SearchOption } from "../components/SearchSelect";
import { ShortcutInput } from "../components/ShortcutInput";

const languageOptions: SearchOption[] = [
  { value: "auto", label: "Automatic (detect)" },
  ...KNOWN_LANGUAGES.map((l) => ({ value: l.code, label: l.name === l.native ? l.name : `${l.name} · ${l.native}` })),
];

/** All languages, or a chosen few, in the overlay's language menu. */
function LanguageFilter({ shown, onChange }: { shown: string[]; onChange: (v: string[]) => void }) {
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState<string[]>([]);
  const [query, setQuery] = useState("");
  const all = shown.length === 0;
  const rows = useMemo(() => {
    const q = query.trim().toLowerCase();
    return KNOWN_LANGUAGES.filter((l) => !q || `${l.name} ${l.native} ${l.code}`.toLowerCase().includes(q));
  }, [query]);
  const edit = () => {
    setDraft(shown);
    setQuery("");
    setOpen(true);
  };
  const toggle = (code: string) => setDraft((d) => (d.includes(code) ? d.filter((c) => c !== code) : [...d, code]));
  return (
    <>
      <Segmented
        label="Languages in the overlay menu"
        value={all ? "all" : "some"}
        options={[
          { value: "all", label: "All" },
          { value: "some", label: shown.length ? `Chosen (${shown.length})` : "Chosen…" },
        ]}
        onChange={(v) => (v === "all" ? onChange([]) : edit())}
      />
      {!all && (
        <button className="btn btn-sm" onClick={edit}>
          <Pencil size={13} /> Edit…
        </button>
      )}
      {open && (
        <Dialog
          title="Languages in the overlay menu"
          onClose={() => setOpen(false)}
          actions={
            <>
              <button className="btn" onClick={() => setOpen(false)}>
                <X size={13} /> Cancel
              </button>
              <button
                className="btn btn-primary"
                disabled={draft.length === 0}
                onClick={() => {
                  onChange(draft);
                  setOpen(false);
                }}
              >
                <Check size={13} /> Save
              </button>
            </>
          }
        >
          <input className="input" placeholder="Search languages…" aria-label="Search languages" value={query} onChange={(e) => setQuery(e.target.value)} />
          <div className="lang-pick">
            {[...rows.filter((l) => draft.includes(l.code)), ...rows.filter((l) => !draft.includes(l.code))].map((l, i, arr) => (
              <Fragment key={l.code}>
                {i > 0 && draft.includes(arr[i - 1].code) && !draft.includes(l.code) && <div className="lang-pick-divider" role="separator" />}
                <label className="lang-pick-item">
                  <input type="checkbox" checked={draft.includes(l.code)} onChange={() => toggle(l.code)} />
                  {l.name === l.native ? l.name : `${l.name} · ${l.native}`}
                </label>
              </Fragment>
            ))}
          </div>
          <p className="row-hint">{draft.length === 0 ? "Pick at least one language, or choose All." : `${draft.length} selected. Automatic is always shown.`}</p>
        </Dialog>
      )}
    </>
  );
}

const percent = (done: number, total: number) => (total > 0 ? Math.min(100, Math.round((done / total) * 100)) : 0);

/** The three Whisper models: pick one or download it. The app never deletes models. */
function ModelCard({ settings, update }: { settings: Settings; update: (m: (d: Settings) => void) => void }) {
  const [models, setModels] = useState<ModelEntry[]>([]);
  const [progress, setProgress] = useState<Record<string, DownloadProgress>>({});
  const [error, setError] = useState<AppErrorPayload | null>(null);
  const [confirmDefault, setConfirmDefault] = useState(false);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const hasNvidia = info?.hasNvidia ?? false;

  const refresh = useCallback(() => {
    void ipc.modelsList().then(setModels).catch((e) => setError(toAppError(e)));
    void ipc.appInfo().then(setInfo);
  }, []);
  useEffect(() => {
    refresh();
    const subs = [
      on("models://changed", refresh),
      on<DownloadProgress>("models://download", (p) => {
        setProgress((cur) => ({ ...cur, [p.modelId]: p }));
        if (p.state === "error" && p.error) setError({ code: "download", detail: p.error });
      }),
    ];
    return () => subs.forEach((s) => void s.then((u) => u()));
  }, [refresh]);

  const installed = models.filter((m) => m.installed);
  // Same rule as the backend: the chosen model if present, else the largest
  // downloaded one, else the first model found in the folder.
  const downloaded = installed.filter((m) => !m.custom);
  const active = installed.find((m) => m.id === settings.model)?.id ?? downloaded[downloaded.length - 1]?.id ?? installed[0]?.id;
  const recommended = hasNvidia ? "whisper-small" : "whisper-base";

  const download = async (id: string) => {
    setError(null);
    try {
      await ipc.modelsDownload([id]);
    } catch (e) {
      setError(toAppError(e));
    }
  };
  const changeFolder = async (dir: string | null) => {
    setError(null);
    try {
      await ipc.modelsSetDir(dir);
      refresh();
    } catch (e) {
      setError(toAppError(e));
    }
  };
  const chooseFolder = async () => {
    const picked = await pickFolder({ directory: true, multiple: false, defaultPath: info?.modelsDir, title: "Folder that holds your Whisper models" });
    if (typeof picked === "string") await changeFolder(picked);
  };
  return (
    <Card title="Speech model" icon={<Brain size={16} />}>
      <p className="row-hint" style={{ margin: "10px 0 4px" }}>
        Whisper runs on this PC; your voice never leaves it. Point the app at a folder that already holds your models, or download them here (checked with SHA-256).
      </p>
      <div className="folder-block">
        <div className="row-label has-icon">
          <span className="row-icon" aria-hidden>
            <FolderOpen size={16} />
          </span>
          <span className="row-label-text">
            Models folder
            <span className="row-hint">Models in this folder, or in its stt sub-folder, are found automatically, so other apps can share them. Nothing is moved or copied.</span>
          </span>
        </div>
        <code className="mono folder-path" title={info?.modelsDir}>
          {info?.modelsDir ?? "…"}
        </code>
        <div className="folder-actions">
          <button className="btn btn-sm" disabled={!info || info.modelsDir === info.defaultModelsDir} onClick={() => setConfirmDefault(true)}>
            <RotateCcw size={13} /> Use default folder
          </button>
          <button className="btn btn-sm btn-primary" onClick={() => void chooseFolder()}>
            <FolderOpen size={13} /> Change folder…
          </button>
        </div>
      </div>
      {installed.length === 0 && (
        <div className="notice warn" style={{ margin: "8px 0" }}>
          No model was found in this folder. Choose a folder that has your models, or download one below to start dictating — “{models.find((m) => m.id === recommended)?.name ?? "Whisper base"}” is a good start for this PC.
        </div>
      )}
      <div className="model-list">
        {models.map((m) => {
          const p = progress[m.id];
          const busy = m.downloading;
          return (
            <div key={m.id} className="model-item">
              <input type="radio" name="model" aria-label={m.name} checked={active === m.id} disabled={!m.installed} onChange={() => update((d) => void (d.model = m.id))} />
              <div style={{ minWidth: 0 }}>
                <div className="row-label">
                  {m.name} {m.id === recommended && !m.custom && <span className="badge accent">Recommended</span>}{m.custom && <span className="badge">Existing</span>}
                </div>
                <span className="row-hint">
                  {m.custom ? m.path : `${m.blurb} · ${formatBytes(m.downloadBytes)}`}
                </span>
                {busy && (
                  <div style={{ marginTop: 6 }}>
                    <div className="progress">
                      <span style={{ width: `${percent(p?.downloadedBytes ?? 0, p?.totalBytes ?? 0)}%` }} />
                    </div>
                    <span className="row-hint">{p?.state === "verifying" ? "Verifying…" : `${formatBytes(p?.downloadedBytes ?? 0)} of ${formatBytes(p?.totalBytes ?? m.downloadBytes)}`}</span>
                  </div>
                )}
              </div>
              {m.installed ? (
                <span />
              ) : busy ? (
                <button className="btn btn-sm" onClick={() => void ipc.modelsCancel(m.id)}>
                  <X size={13} /> Cancel
                </button>
              ) : (
                <button className="btn btn-sm btn-primary" onClick={() => void download(m.id)}>
                  <Download size={13} /> Download
                </button>
              )}
            </div>
          );
        })}
      </div>
      {error && <ErrorNotice error={error} actions={<button className="btn btn-sm" onClick={() => setError(null)}><X size={13} /> Dismiss</button>} />}
      {confirmDefault && (
        <Dialog
          title="Use the app's own models folder?"
          onClose={() => setConfirmDefault(false)}
          actions={
            <>
              <button className="btn" onClick={() => setConfirmDefault(false)}>
                <X size={13} /> Cancel
              </button>
              <button
                className="btn btn-primary"
                onClick={() => {
                  setConfirmDefault(false);
                  void changeFolder(null);
                }}
              >
                <RotateCcw size={13} /> Use default folder
              </button>
            </>
          }
        >
          <p>
            Dictation will switch to <code className="mono">{info?.defaultModelsDir}</code>. Your models in <code className="mono">{info?.modelsDir}</code> are not touched, but they will not be listed until you pick that folder again.
          </p>
        </Dialog>
      )}
    </Card>
  );
}

/** CPU or GPU. The CUDA libraries are a separate download because they are far larger than the app. */
function ProcessingCard({ settings }: { settings: Settings }) {
  const [status, setStatus] = useState<GpuStatus | null>(null);
  const [progress, setProgress] = useState<GpuProgress | null>(null);
  const [error, setError] = useState<AppErrorPayload | null>(null);

  const refresh = useCallback(() => void ipc.gpuStatus().then(setStatus).catch((e) => setError(toAppError(e))), []);
  useEffect(() => {
    refresh();
    const subs = [on("gpu://changed", refresh), on<GpuProgress>("gpu://download", setProgress)];
    return () => subs.forEach((s) => void s.then((u) => u()));
  }, [refresh]);

  const busy = progress?.state === "downloading" || progress?.state === "extracting";
  const wantGpu = settings.device === "gpu";
  const needsRestart = !!status && (status.restartRequired || (status.active && !wantGpu));
  const act = async (f: () => Promise<void>) => {
    setError(null);
    try {
      await f();
    } catch (e) {
      setProgress(null);
      setError(toAppError(e));
    }
  };

  return (
    <Card title="Processing" icon={<Cpu size={16} />}>
      <Row label="Run speech recognition on" icon={<Cpu size={16} />} hint={status?.supported ? undefined : "No NVIDIA graphics card was found, so only the CPU is available."}>
        <Segmented
          label="Device"
          value={wantGpu && status?.supported ? "gpu" : "cpu"}
          options={[
            { value: "cpu", label: "CPU", icon: <Cpu size={13} /> },
            { value: "gpu", label: "GPU (NVIDIA)", icon: <Zap size={13} />, disabled: !status?.supported },
          ]}
          onChange={(v) => void act(() => ipc.gpuSetEnabled(v === "gpu"))}
        />
        <span className="badge">
          {status?.active ? (
            <>
              <Zap size={12} /> Running on {status.gpuName ?? "GPU"}
            </>
          ) : (
            <>
              <Cpu size={12} /> Running on CPU
            </>
          )}
        </span>
      </Row>
      {status?.supported && wantGpu && !status.installed && (
        <div className="notice info" style={{ margin: "8px 0" }}>
          <span style={{ flex: 1 }}>
            {status.reusable
              ? "GPU mode needs the CUDA libraries. Open Local Assistant already has them, so they can be linked here without downloading again."
              : `GPU mode needs the CUDA libraries (${formatBytes(status.downloadBytes)}, downloaded once). Until then dictation runs on the CPU.`}
          </span>
          {busy ? (
            <button className="btn btn-sm" onClick={() => void ipc.gpuCancel()}>
              <X size={13} /> Cancel
            </button>
          ) : (
            <button className="btn btn-sm btn-primary" onClick={() => void act(() => ipc.gpuInstall())}>
              <Download size={13} /> {status.reusable ? "Use existing" : "Download"}
            </button>
          )}
        </div>
      )}
      {busy && progress && (
        <div style={{ margin: "6px 0 10px" }}>
          <div className="progress">
            <span style={{ width: `${percent(progress.downloadedBytes, progress.totalBytes)}%` }} />
          </div>
          <span className="row-hint">{progress.state === "extracting" ? "Unpacking…" : `${formatBytes(progress.downloadedBytes)} of ${formatBytes(progress.totalBytes)}`}</span>
        </div>
      )}
      {progress?.state === "error" && progress.error && <p className="row-hint">{progress.error}</p>}
      {needsRestart && (
        <div className="notice warn" style={{ margin: "8px 0" }}>
          <span style={{ flex: 1 }}>The change to {wantGpu ? "GPU" : "CPU"} takes effect after a restart.</span>
          <button className="btn btn-sm btn-primary" onClick={() => void act(() => ipc.appRestart())}>
            <RotateCw size={13} /> Restart now
          </button>
        </div>
      )}
      {status?.installed && (
        <Row label="CUDA libraries" icon={<Zap size={16} />} hint={`Installed, ${formatBytes(status.sizeBytes)}`} end>
          <span className="badge ok">Installed</span>
        </Row>
      )}
      {error && <ErrorNotice error={error} actions={<button className="btn btn-sm" onClick={() => setError(null)}><X size={13} /> Dismiss</button>} />}
    </Card>
  );
}

type Tab = "model" | "processing" | "dictation" | "overlay" | "app";
const TABS: { id: Tab; label: string; icon: ReactNode }[] = [
  { id: "model", label: "Speech model", icon: <Brain size={15} /> },
  { id: "processing", label: "Processing", icon: <Cpu size={15} /> },
  { id: "dictation", label: "Dictation", icon: <Mic size={15} /> },
  { id: "overlay", label: "Overlay", icon: <PictureInPicture2 size={15} /> },
  { id: "app", label: "App", icon: <SlidersHorizontal size={15} /> },
];

export function SettingsPage() {
  const [tab, setTab] = useState<Tab>("model");
  const [confirmPosition, setConfirmPosition] = useState(false);
  const settings = useSettings((s) => s.settings);
  const update = useSettings((s) => s.update);
  const saveError = useSettings((s) => s.error);
  const [mics, setMics] = useState<AudioDevice[]>([]);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [shortcutErrors, setShortcutErrors] = useState<string[]>([]);

  useEffect(() => {
    void ipc.audioInputs().then(setMics).catch(() => undefined);
    void ipc.appInfo().then(setInfo);
    void ipc.shortcutErrors().then(setShortcutErrors);
  }, []);
  useEffect(() => {
    void ipc.shortcutErrors().then(setShortcutErrors);
  }, [settings?.shortcut]);

  if (!settings) return null;
  const set = (m: (d: Settings) => void) => void update(m);

  return (
    <div className="settings-shell">
      <header className="titlebar" data-tauri-drag-region>
        <span className="titlebar-title" data-tauri-drag-region>
          <img className="app-icon-sm" src="/app-icon.png" alt="" aria-hidden /> Open Local Dictation
        </span>
        <div className="titlebar-buttons">
          <button type="button" className="icon-btn" aria-label="Minimize" title="Minimize" onClick={() => void ipc.settingsMinimize()}>
            <Minus size={14} />
          </button>
          <button type="button" className="icon-btn close" aria-label="Close" title="Close (keeps running in the tray)" onClick={() => void ipc.settingsClose()}>
            <X size={14} />
          </button>
        </div>
      </header>
      <main className="settings-main">
        <div className="brand">
          <span className="brand-mark" aria-hidden>
            <img src="/app-icon.png" alt="" />
          </span>
          <div>
            <h1>Open Local Dictation</h1>
            <p className="settings-intro" style={{ margin: 0 }}>
              Hold {settings.shortcut.replace("CommandOrControl", "Ctrl").replaceAll("+", " + ")}, speak, and the text lands in the app you were using.
            </p>
          </div>
        </div>
        {saveError && <ErrorNotice error={saveError} />}
        {shortcutErrors.length > 0 && (
          <div className="notice warn" style={{ maxWidth: 820, marginBottom: 12 }}>
            <div>
              The shortcut could not be registered — another app may be using it. Pick a different one below.
              <ul style={{ margin: "4px 0 0", paddingInlineStart: 18 }}>
                {shortcutErrors.map((e) => (
                  <li key={e}>{e}</li>
                ))}
              </ul>
            </div>
          </div>
        )}

        <div className="tabs" role="tablist" aria-label="Settings sections">
          {TABS.map((t) => (
            <button key={t.id} type="button" role="tab" id={`tab-${t.id}`} aria-selected={tab === t.id} aria-controls={`panel-${t.id}`} className="tab" onClick={() => setTab(t.id)}>
              {t.icon}
              {t.label}
            </button>
          ))}
        </div>

        <div role="tabpanel" id={`panel-${tab}`} aria-labelledby={`tab-${tab}`}>
        {tab === "model" && <ModelCard settings={settings} update={set} />}
        {tab === "processing" && <ProcessingCard settings={settings} />}

        {tab === "dictation" && (
        <Card title="Dictation" icon={<Mic size={16} />}>
          <Row label="Shortcut" icon={<Keyboard size={16} />}>
            <ShortcutInput label="Dictation shortcut" value={settings.shortcut} defaultValue="CommandOrControl+Alt+Space" onChange={(v) => set((d) => void (d.shortcut = v))} />
          </Row>
          <Row label="Shortcut behavior" icon={<MousePointerClick size={16} />}>
            <Segmented
              label="Shortcut behavior"
              value={settings.mode}
              options={[
                { value: "hold", label: "Hold to talk" },
                { value: "toggle", label: "Press to start / stop" },
              ]}
              onChange={(v) => set((d) => void (d.mode = v))}
            />
          </Row>
          <Row label="Language" icon={<Languages size={16} />} hint={settings.shownLanguages?.length ? "Only your chosen languages are listed. Change them below." : "You can also change it in the overlay while dictating."}>
<SearchSelect
              label="Language"
              placeholder="Search languages…"
              value={settings.language}
              options={settings.shownLanguages?.length ? languageOptions.filter((o) => o.value === "auto" || o.value === settings.language || settings.shownLanguages.includes(o.value)) : languageOptions}
              onChange={(v) => set((d) => void (d.language = v))}
            />
          </Row>
          <Row label="Languages in the overlay menu" icon={<ListFilter size={16} />}>
            <LanguageFilter shown={settings.shownLanguages ?? []} onChange={(v) => set((d) => void (d.shownLanguages = v))} />
          </Row>
          <Row label="Microphone" icon={<Mic size={16} />}>
<SearchSelect
              label="Microphone"
              placeholder="Search microphones…"
              value={settings.microphone ?? ""}
              options={[{ value: "", label: "System default" }, ...mics.map((m) => ({ value: m.id, label: `${m.name}${m.isDefault ? " (default)" : ""}` }))]}
              onChange={(v) => set((d) => void (d.microphone = v || null))}
            />
          </Row>
          <Row label="Review before inserting" icon={<PenLine size={16} />} hint="Opens the text in the overlay so you can edit it, then Ctrl+Enter inserts it. Off: words are typed live as you speak." htmlFor="sw-review">
            <Switch id="sw-review" label="Review before inserting" checked={settings.reviewBeforeInsert} onChange={(v) => set((d) => void (d.reviewBeforeInsert = v))} />
          </Row>
          <Row label="Insert by" icon={<Type size={16} />}>
            <Segmented
              label="Insert by"
              value={settings.insertMethod}
              options={[
                { value: "type", label: "Typing", icon: <Keyboard size={13} /> },
                { value: "paste", label: "Pasting", icon: <ClipboardPaste size={13} /> },
              ]}
              onChange={(v) => set((d) => void (d.insertMethod = v))}
            />
          </Row>
          <Row label="Add a space after the text" icon={<Space size={16} />} htmlFor="sw-space">
            <Switch id="sw-space" label="Add a space after the text" checked={settings.addTrailingSpace} onChange={(v) => set((d) => void (d.addTrailingSpace = v))} />
          </Row>
        </Card>
        )}

        {tab === "overlay" && (
        <Card title="Overlay" icon={<PictureInPicture2 size={16} />}>
          <Row label="Pin the overlay" icon={<Pin size={16} />} hint="Keeps the overlay on screen, always on top of other apps. It listens to nothing until you press the shortcut or its mic button." htmlFor="sw-pin">
            <Switch id="sw-pin" label="Pin the overlay" checked={settings.overlayPinned} onChange={(v) => void ipc.overlaySetPinned(v)} />
          </Row>
          <Row label="Overlay size" icon={<Maximize2 size={16} />} hint="Small is a compact pill: what you say shows above it and review edits there too. With both, a button in the overlay switches between them.">
            <Segmented
              label="Overlay size"
              value={settings.overlayLayouts}
              options={[
                { value: "small", label: "Small only" },
                { value: "large", label: "Large only" },
                { value: "both", label: "Both" },
              ]}
              onChange={(v) => set((d) => void (d.overlayLayouts = v))}
            />
          </Row>
          <Row label="Settings button" icon={<SettingsIcon size={16} />} hint="Shows a gear in the overlay that opens these settings." htmlFor="sw-gear">
            <Switch id="sw-gear" label="Settings button" checked={settings.overlayShowSettings} onChange={(v) => set((d) => void (d.overlayShowSettings = v))} />
          </Row>
          <Row label="Overlay opacity" icon={<Droplets size={16} />} hint="Makes the overlay background see-through. Text and buttons stay solid.">
            <div style={{ display: "flex", alignItems: "center", gap: 10, width: 220 }}>
              <input
                type="range"
                min={30}
                max={100}
                step={5}
                aria-label="Overlay opacity"
                value={settings.overlayOpacity}
                onChange={(e) => set((d) => void (d.overlayOpacity = Number(e.target.value)))}
              />
              <span style={{ width: 40, textAlign: "right", fontVariantNumeric: "tabular-nums" }}>{settings.overlayOpacity}%</span>
            </div>
          </Row>
          <Row label="Lock the small overlay's size" icon={<Lock size={16} />} hint="Off: drag the right edge of the small overlay to change its width (double-click the edge to reset it)." htmlFor="sw-lock">
            <Switch id="sw-lock" label="Lock the small overlay's size" checked={settings.overlaySizeLocked} onChange={(v) => set((d) => void (d.overlaySizeLocked = v))} />
          </Row>
          <Row label="Move sideways only" icon={<MoveHorizontal size={16} />} hint="The overlay slides left and right and stays just above the taskbar." htmlFor="sw-side">
            <Switch id="sw-side" label="Move sideways only" checked={settings.overlayHorizontalOnly} onChange={(v) => set((d) => void (d.overlayHorizontalOnly = v))} />
          </Row>
          <Row label="Lock the overlay position" icon={<LockKeyhole size={16} />} hint="Stops the overlay from being dragged." htmlFor="sw-poslock">
            <Switch id="sw-poslock" label="Lock the overlay position" checked={settings.overlayPositionLocked} onChange={(v) => set((d) => void (d.overlayPositionLocked = v))} />
          </Row>
          <Row label="Overlay position" icon={<Move size={16} />} hint="Drag the overlay by its header to move it (unless the position is locked).">
            <button className="btn btn-sm" onClick={() => setConfirmPosition(true)}>
              <RotateCcw size={13} /> Reset position
            </button>
            {confirmPosition && (
              <Dialog
                title="Reset the overlay position?"
                onClose={() => setConfirmPosition(false)}
                actions={
                  <>
                    <button className="btn" onClick={() => setConfirmPosition(false)}>
                      <X size={13} /> Cancel
                    </button>
                    <button
                      className="btn btn-primary"
                      onClick={() => {
                        setConfirmPosition(false);
                        void ipc.dictationResetOverlayPosition();
                      }}
                    >
                      <RotateCcw size={13} /> Reset position
                    </button>
                  </>
                }
              >
                <p>The overlay goes back to its default spot at the bottom centre of the screen.</p>
              </Dialog>
            )}
          </Row>
        </Card>
        )}

        {tab === "app" && (
        <Card title="App" icon={<SlidersHorizontal size={16} />}>
          <Row label="Start with Windows" icon={<Power size={16} />} htmlFor="sw-start">
            <Switch id="sw-start" label="Start with Windows" checked={settings.startWithOs} onChange={(v) => set((d) => void (d.startWithOs = v))} />
          </Row>
          <Row label="Theme" icon={<Palette size={16} />}>
            <Segmented
              label="Theme"
              value={settings.theme}
              options={[
                { value: "system", label: "System", icon: <Monitor size={13} /> },
                { value: "light", label: "Light", icon: <Sun size={13} /> },
                { value: "dark", label: "Dark", icon: <Moon size={13} /> },
              ]}
              onChange={(v) => set((d) => void (d.theme = v))}
            />
          </Row>
          <Row label="Logs" icon={<FileText size={16} />} hint={info ? `Version ${info.version}. Logs never contain what you dictate.` : undefined} end>
            <button className="btn btn-sm" onClick={() => void ipc.openLogs()}>
              <FolderOpen size={13} /> Open logs
            </button>
          </Row>
        </Card>
        )}
        </div>
      </main>
    </div>
  );
}
