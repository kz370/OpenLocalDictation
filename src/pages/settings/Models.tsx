import { useCallback, useEffect, useState } from "react";
import { open as pickFolder } from "@tauri-apps/plugin-dialog";
import { Brain, Check, ChevronDown, ChevronRight, Cpu, Download, Eye, EyeOff, FolderOpen, Plus, Power, RotateCcw, RotateCw, X, Zap } from "lucide-react";
import { ipc, on, toAppError } from "../../ipc";
import { formatBytes } from "../../strings";
import type { AppErrorPayload, AppInfo, DownloadProgress, GpuProgress, GpuStatus, ModelEntry, Settings } from "../../types";
import { Dialog, ErrorNotice, Segmented, Switch } from "../../components/controls";
import { Card, Row } from "../../components/layout";
import { RowMenu } from "../../components/RowMenu";
import { deviceLabel, percent, resolveActiveModel, shortModel, useLoadedModels, type Update } from "./shared";

/** The Whisper models: pick one, load it, or download it. The app never deletes models. */
function ModelCard({ settings, update }: { settings: Settings; update: Update }) {
  const [models, setModels] = useState<ModelEntry[] | null>(null);
  const [progress, setProgress] = useState<Record<string, DownloadProgress>>({});
  const [error, setError] = useState<AppErrorPayload | null>(null);
  const [confirmDefault, setConfirmDefault] = useState(false);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [loaded, pollLoaded] = useLoadedModels();
  const [loadingId, setLoadingId] = useState<string | null>(null);
  const [showHidden, setShowHidden] = useState(false);
  const [foldersOpen, setFoldersOpen] = useState(false);
  const hasNvidia = info?.hasNvidia ?? false;

  const refresh = useCallback(() => {
    void ipc
      .modelsList()
      .then(setModels)
      .catch((e) => {
        setModels((m) => m ?? []);
        setError(toAppError(e));
      });
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

  const all = models ?? [];
  const installed = all.filter((m) => m.installed);
  const active = resolveActiveModel(all, settings);
  const recommended = hasNvidia ? "whisper-small" : "whisper-base";
  const hiddenIds = settings.hiddenModels ?? [];
  const shownModels = all.filter((m) => !hiddenIds.includes(m.id));
  const hiddenModels = all.filter((m) => hiddenIds.includes(m.id));
  const setHidden = (id: string, hide: boolean) => update((d) => void (d.hiddenModels = hide ? [...new Set([...(d.hiddenModels ?? []), id])] : (d.hiddenModels ?? []).filter((x) => x !== id)));
  // Same rule as the backend when the user never chose: the default model and the one for the dictation language.
  const languageModelId = installed.find((m) => m.id === settings.languageModels?.[settings.language])?.id;
  const autoIds: string[] = settings.autoLoadModels ?? [...new Set([active, languageModelId].filter((x): x is string => !!x))];
  const setAuto = (id: string, on: boolean) => update((d) => void (d.autoLoadModels = on ? [...new Set([...autoIds, id])] : autoIds.filter((x) => x !== id)));
  const foldersShown = foldersOpen || (models !== null && installed.length === 0);

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
  const load = async (id: string) => {
    setError(null);
    setLoadingId(id);
    try {
      await ipc.sttLoad(id);
    } catch (e) {
      setError(toAppError(e));
    } finally {
      setLoadingId(null);
      pollLoaded();
    }
  };
  const unload = async (id: string) => {
    await ipc.sttUnload(id);
    pollLoaded();
  };
  const addOwn = async () => {
    setError(null);
    const picked = await pickFolder({ directory: true, multiple: false, title: "Folder of the model you want to add (unpacked from a sherpa-onnx download)" });
    if (typeof picked !== "string") return;
    try {
      await ipc.modelsAddOwn(picked);
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
        Whisper runs on this PC; your voice never leaves it. Downloads are checked with SHA-256.
      </p>
      {models !== null && installed.length === 0 && (
        <div className="notice warn" style={{ margin: "8px 0" }}>
          No model was found. Download one below to start dictating — “{all.find((m) => m.id === recommended)?.name ?? "Whisper base"}” is a good start for this PC.
        </div>
      )}
      {models === null && (
        <>
          <div className="model-skeleton" />
          <div className="model-skeleton" />
        </>
      )}
      <div className="model-list">
        {shownModels.map((m) => {
          const p = progress[m.id];
          const busy = m.downloading;
          const isActive = active === m.id;
          const l = loaded.find((x) => x.id === m.id);
          return (
            <div key={m.id} className={`model-item${isActive ? " active" : ""}`}>
              <div style={{ minWidth: 0 }}>
                <div className="model-name">
                  {shortModel(m.name)}
                  {isActive && (
                    <span className="badge accent">
                      <Check size={11} /> In use
                    </span>
                  )}
                  {m.id === recommended && !m.custom && <span className="badge">Recommended</span>}
                  {m.custom && (
                    <span className="badge">
                      {m.blurb === "Added by you" ? "Yours" : "Existing"} · {m.kind}
                    </span>
                  )}
                  {l && (
                    <span className="badge ok" title={l.gpuBlocked ? "The GPU is on, but this model's file format (.ort) cannot run on it." : undefined}>
                      Loaded · {deviceLabel(l)}
                    </span>
                  )}
                </div>
                <span className="row-hint">{m.custom ? m.path : `${m.blurb} · ${formatBytes(m.downloadBytes)}`}</span>
                {busy && (
                  <div style={{ marginTop: 6 }}>
                    <div className="progress">
                      <span style={{ width: `${percent(p?.downloadedBytes ?? 0, p?.totalBytes ?? 0)}%` }} />
                    </div>
                    <span className="row-hint">{p?.state === "verifying" ? "Verifying…" : `${formatBytes(p?.downloadedBytes ?? 0)} of ${formatBytes(p?.totalBytes ?? m.downloadBytes)}`}</span>
                  </div>
                )}
              </div>
              <div className="model-actions">
                {m.installed ? (
                  <>
                    {!isActive && (
                      <button className="btn btn-sm" onClick={() => update((d) => void (d.model = m.id))} title="Use this model for dictation (languages with their own model keep it).">
                        <Check size={13} /> Use
                      </button>
                    )}
                    {l ? (
                      <button className="btn btn-sm" onClick={() => void unload(m.id)} title="Free this model's memory. Nothing is deleted; it loads again when a dictation needs it.">
                        <Power size={13} /> Unload
                      </button>
                    ) : (
                      <button className="btn btn-sm" disabled={loadingId !== null} onClick={() => void load(m.id)} title="Load this model into memory now, so the next dictation does not wait for it.">
                        {loadingId === m.id ? <span className="spinner" /> : <Zap size={13} />} {loadingId === m.id ? "Loading…" : "Load"}
                      </button>
                    )}
                  </>
                ) : busy ? (
                  <button className="btn btn-sm" onClick={() => void ipc.modelsCancel(m.id)}>
                    <X size={13} /> Cancel
                  </button>
                ) : (
                  <button className="btn btn-sm btn-primary" onClick={() => void download(m.id)}>
                    <Download size={13} /> Download
                  </button>
                )}
                <RowMenu label={`More for ${shortModel(m.name)}`}>
                  {m.installed && (
                    <label className="row-menu-item" title="Load this model into memory every time the app starts">
                      Load when the app starts
                      <Switch id={`auto-${m.id}`} label={`Load ${shortModel(m.name)} when the app starts`} checked={autoIds.includes(m.id)} onChange={(v) => setAuto(m.id, v)} />
                    </label>
                  )}
                  <button
                    type="button"
                    role="menuitem"
                    className="row-menu-item"
                    disabled={isActive || busy}
                    title={isActive ? "This is the model in use; choose another before hiding it." : "Hide from the list. Nothing is deleted; it can be shown again."}
                    onClick={() => setHidden(m.id, true)}
                  >
                    Hide from the list <EyeOff size={14} />
                  </button>
                </RowMenu>
              </div>
            </div>
          );
        })}
      </div>
      {hiddenModels.length > 0 && (
        <div className="hidden-models">
          <button className="hidden-toggle" aria-expanded={showHidden} onClick={() => setShowHidden((v) => !v)}>
            {showHidden ? <ChevronDown size={14} /> : <ChevronRight size={14} />} Hidden models ({hiddenModels.length})
          </button>
          {showHidden &&
            hiddenModels.map((m) => (
              <div key={m.id} className="hidden-item">
                <span>
                  {shortModel(m.name)}{" "}
                  <span className="row-hint" style={{ display: "inline" }}>
                    · {m.kind}
                    {m.installed ? "" : " · not downloaded"}
                  </span>
                </span>
                <button className="btn btn-sm" onClick={() => setHidden(m.id, false)}>
                  <Eye size={13} /> Show
                </button>
              </div>
            ))}
        </div>
      )}
      <button className="disclosure" aria-expanded={foldersShown} onClick={() => setFoldersOpen((v) => !v)}>
        {foldersShown ? <ChevronDown size={14} /> : <ChevronRight size={14} />} Folder &amp; your own models
      </button>
      {foldersShown && (
        <>
          <div className="folder-block">
            <div className="row-label has-icon">
              <span className="row-icon" aria-hidden>
                <FolderOpen size={16} />
              </span>
              <span className="row-label-text">
                Models folder
                <span className="row-hint">Models here (or in its stt sub-folder) are found automatically. Nothing is moved or copied.</span>
              </span>
            </div>
            <code className="mono folder-path" title={info?.modelsDir}>
              {info?.modelsDir ?? "Loading…"}
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
          <div className="folder-block">
            <div className="row-label has-icon">
              <span className="row-icon" aria-hidden>
                <Plus size={16} />
              </span>
              <span className="row-label-text">
                Your own model
                <span className="row-hint">Any sherpa-onnx model folder (Whisper, Moonshine, transducer, NeMo, SenseVoice…). Used where it is, never copied or deleted.</span>
                <span className="row-hint">For Arabic only, “sherpa-onnx-moonshine-base-ar-quantized” (about 120 MB) is much smaller than a large Whisper.</span>
              </span>
            </div>
            <div className="folder-actions">
              <button className="btn btn-sm btn-primary" onClick={() => void addOwn()}>
                <Plus size={13} /> Add your own model…
              </button>
            </div>
          </div>
        </>
      )}
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

export function ModelsPage({ settings, update }: { settings: Settings; update: Update }) {
  return (
    <>
      <ModelCard settings={settings} update={update} />
      <ProcessingCard settings={settings} />
    </>
  );
}
