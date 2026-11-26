import { useCallback, useEffect, useState } from "react";
import { open as pickFolder } from "@tauri-apps/plugin-dialog";
import { Cpu, Download, FolderOpen, Mic, Minus, RotateCcw, RotateCw, X, Zap } from "lucide-react";
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
    <Card title="Speech model">
      <p className="row-hint" style={{ margin: "10px 0 4px" }}>
        Whisper runs on this PC; your voice never leaves it. Point the app at a folder that already holds your models, or download them here (checked with SHA-256).
      </p>
      <div className="folder-block">
        <div className="row-label">
          Models folder
          <span className="row-hint">Models in this folder, or in its stt sub-folder, are found automatically, so other apps can share them. Nothing is moved or copied.</span>
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
                  Cancel
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
      {error && <ErrorNotice error={error} actions={<button className="btn btn-sm" onClick={() => setError(null)}>Dismiss</button>} />}
      {confirmDefault && (
        <Dialog
          title="Use the app's own models folder?"
          onClose={() => setConfirmDefault(false)}
          actions={
            <>
              <button className="btn" onClick={() => setConfirmDefault(false)}>
                Cancel
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
    <Card title="Processing">
      <Row label="Run speech recognition on" hint={status?.supported ? undefined : "No NVIDIA graphics card was found, so only the CPU is available."}>
        <Segmented
          label="Device"
          value={wantGpu && status?.supported ? "gpu" : "cpu"}
          options={[
            { value: "cpu", label: "CPU" },
            { value: "gpu", label: "GPU (NVIDIA)", disabled: !status?.supported },
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
              Cancel
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
        <Row label="CUDA libraries" hint={`Installed, ${formatBytes(status.sizeBytes)}`} end>
          <span className="badge ok">Installed</span>
        </Row>
      )}
      {error && <ErrorNotice error={error} actions={<button className="btn btn-sm" onClick={() => setError(null)}>Dismiss</button>} />}
    </Card>
  );
}

type Tab = "model" | "processing" | "dictation" | "app";
const TABS: { id: Tab; label: string }[] = [
  { id: "model", label: "Speech model" },
  { id: "processing", label: "Processing" },
  { id: "dictation", label: "Dictation" },
  { id: "app", label: "App" },
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
          <Mic size={14} aria-hidden /> Open Local Dictation
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
            <Mic size={18} />
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
              {t.label}
            </button>
          ))}
        </div>

        <div role="tabpanel" id={`panel-${tab}`} aria-labelledby={`tab-${tab}`}>
        {tab === "model" && <ModelCard settings={settings} update={set} />}
        {tab === "processing" && <ProcessingCard settings={settings} />}

        {tab === "dictation" && (
        <Card title="Dictation">
          <Row label="Shortcut">
            <ShortcutInput label="Dictation shortcut" value={settings.shortcut} defaultValue="CommandOrControl+Alt+Space" onChange={(v) => set((d) => void (d.shortcut = v))} />
          </Row>
          <Row label="Shortcut behavior">
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
          <Row label="Language" hint="You can also change it in the overlay while dictating.">
<SearchSelect
              label="Language"
              placeholder="Search languages…"
              value={settings.language}
              options={languageOptions}
              onChange={(v) => set((d) => void (d.language = v))}
            />
          </Row>
          <Row label="Microphone">
<SearchSelect
              label="Microphone"
              placeholder="Search microphones…"
              value={settings.microphone ?? ""}
              options={[{ value: "", label: "System default" }, ...mics.map((m) => ({ value: m.id, label: `${m.name}${m.isDefault ? " (default)" : ""}` }))]}
              onChange={(v) => set((d) => void (d.microphone = v || null))}
            />
          </Row>
          <Row label="Review before inserting" hint="Opens the text in the overlay so you can edit it, then Ctrl+Enter inserts it. Off: words are typed live as you speak." htmlFor="sw-review">
            <Switch id="sw-review" label="Review before inserting" checked={settings.reviewBeforeInsert} onChange={(v) => set((d) => void (d.reviewBeforeInsert = v))} />
          </Row>
          <Row label="Insert by">
            <Segmented
              label="Insert by"
              value={settings.insertMethod}
              options={[
                { value: "type", label: "Typing" },
                { value: "paste", label: "Pasting" },
              ]}
              onChange={(v) => set((d) => void (d.insertMethod = v))}
            />
          </Row>
          <Row label="Add a space after the text" htmlFor="sw-space">
            <Switch id="sw-space" label="Add a space after the text" checked={settings.addTrailingSpace} onChange={(v) => set((d) => void (d.addTrailingSpace = v))} />
          </Row>
          <Row label="Overlay position" hint="Drag the overlay by its header to move it.">
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
                      Cancel
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
        <Card title="App">
          <Row label="Start with Windows" htmlFor="sw-start">
            <Switch id="sw-start" label="Start with Windows" checked={settings.startWithOs} onChange={(v) => set((d) => void (d.startWithOs = v))} />
          </Row>
          <Row label="Theme">
            <Segmented
              label="Theme"
              value={settings.theme}
              options={[
                { value: "system", label: "System" },
                { value: "light", label: "Light" },
                { value: "dark", label: "Dark" },
              ]}
              onChange={(v) => set((d) => void (d.theme = v))}
            />
          </Row>
          <Row label="Logs" hint={info ? `Version ${info.version}. Logs never contain what you dictate.` : undefined} end>
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
