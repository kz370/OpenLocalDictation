import { useCallback, useEffect, useState } from "react";
import { Brain, Download, Keyboard, Languages, Mic, Square } from "lucide-react";
import { ipc, on, toAppError } from "../../ipc";
import { formatBytes } from "../../strings";
import type { AppErrorPayload, AppInfo, AudioDevice, DownloadProgress, GpuStatus, ModelEntry, Settings } from "../../types";
import { ErrorNotice } from "../../components/controls";
import { prettyAccelerator } from "../../components/ShortcutInput";
import { VoiceBars } from "../../components/VoiceBars";
import { langLabel, percent, resolveActiveModel } from "./shared";

type Go = (section: "models" | "languages" | "dictation") => void;

interface MicLevel {
  value: number;
  bands: number[];
}

/** Level meter for the chosen microphone. Opens the microphone only while the test runs. */
function MicTest({ micName }: { micName: string }) {
  const [running, setRunning] = useState(false);
  const [level, setLevel] = useState<MicLevel>({ value: 0, bands: [] });
  const [error, setError] = useState<AppErrorPayload | null>(null);

  const stop = useCallback(() => {
    setRunning(false);
    setLevel({ value: 0, bands: [] });
    void ipc.micTestStop().catch(() => undefined);
  }, []);
  const start = async () => {
    setError(null);
    try {
      await ipc.micTestStart();
      setRunning(true);
    } catch (e) {
      setError(toAppError(e));
    }
  };

  useEffect(() => {
    const sub = on<MicLevel>("mic://level", setLevel);
    // The microphone is never left open behind a hidden window or another page.
    const hidden = () => document.hidden && stop();
    document.addEventListener("visibilitychange", hidden);
    window.addEventListener("blur", stop);
    return () => {
      void sub.then((u) => u());
      document.removeEventListener("visibilitychange", hidden);
      window.removeEventListener("blur", stop);
      void ipc.micTestStop().catch(() => undefined);
    };
  }, [stop]);

  return (
    <>
      <div className="mic-test">
        {running ? <VoiceBars bands={level.bands} level={level.value} label="Microphone level" /> : <span className="row-hint" style={{ flex: 1, margin: 0 }}>Say something to see the level move. Nothing is recorded or saved.</span>}
        <button className={`btn btn-sm${running ? "" : " btn-primary"}`} onClick={() => (running ? stop() : void start())}>
          {running ? <Square size={12} /> : <Mic size={13} />} {running ? "Stop test" : "Test microphone"}
        </button>
      </div>
      <span className="row-hint" style={{ margin: 0 }}>
        {micName}
      </span>
      {error && <ErrorNotice error={error} />}
    </>
  );
}

export function HomePage({ settings, info, gpu, mics, go }: { settings: Settings; info: AppInfo | null; gpu: GpuStatus | null; mics: AudioDevice[]; go: Go }) {
  const [models, setModels] = useState<ModelEntry[] | null>(null);
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [error, setError] = useState<AppErrorPayload | null>(null);

  useEffect(() => {
    const refresh = () => void ipc.modelsList().then(setModels).catch(() => setModels((m) => m ?? []));
    refresh();
    const subs = [
      on("models://changed", refresh),
      on<DownloadProgress>("models://download", (p) => {
        setProgress(p);
        if (p.state === "error" && p.error) setError({ code: "download", detail: p.error });
      }),
    ];
    return () => subs.forEach((s) => void s.then((u) => u()));
  }, []);

  const all = models ?? [];
  const activeId = resolveActiveModel(all, settings);
  const active = all.find((m) => m.id === activeId);
  const recommendedId = info?.hasNvidia ? "whisper-small" : "whisper-base";
  const recommended = all.find((m) => m.id === recommendedId);
  const downloading = all.find((m) => m.downloading);
  const mic = settings.microphone ? (mics.find((m) => m.id === settings.microphone)?.name ?? "Chosen microphone") : (mics.find((m) => m.isDefault)?.name ?? "System default");
  const device = gpu?.active ? `GPU · ${gpu.gpuName ?? "NVIDIA"}` : "CPU";

  const download = async () => {
    setError(null);
    try {
      await ipc.modelsDownload([recommendedId]);
    } catch (e) {
      setError(toAppError(e));
    }
  };

  return (
    <>
      {models !== null && !active && (
        <div className="hero">
          <Brain size={28} aria-hidden />
          <div>
            <strong>Download a speech model to start dictating</strong>
            <span className="row-hint" style={{ color: "inherit", opacity: 0.85 }}>
              {recommended ? `${recommended.name} (${formatBytes(recommended.downloadBytes)}) is a good start for this PC.` : "Pick one on the Models page."} It runs on this PC; your voice never leaves it.
            </span>
            {downloading && (
              <div style={{ marginTop: 8 }}>
                <div className="progress">
                  <span style={{ width: `${percent(progress?.downloadedBytes ?? 0, progress?.totalBytes ?? 0)}%` }} />
                </div>
              </div>
            )}
          </div>
          {downloading ? (
            <button className="btn" onClick={() => void ipc.modelsCancel(downloading.id)}>
              Cancel
            </button>
          ) : recommended ? (
            <button className="btn btn-primary" onClick={() => void download()}>
              <Download size={14} /> Download
            </button>
          ) : (
            <button className="btn btn-primary" onClick={() => go("models")}>
              Open Models
            </button>
          )}
        </div>
      )}
      {error && <ErrorNotice error={error} />}
      <div className="home-grid">
        <section className="status-card" aria-label="Speech model">
          <div className="status-label">
            <Brain size={14} /> Speech model
          </div>
          <div className="status-value">{models === null ? "…" : active ? active.name : "None yet"}</div>
          <span className="row-hint" style={{ margin: 0 }}>
            Runs on {device}
          </span>
          <div className="status-foot">
            <span />
            <button className="btn btn-sm" onClick={() => go("models")}>
              Change
            </button>
          </div>
        </section>
        <section className="status-card" aria-label="Shortcut">
          <div className="status-label">
            <Keyboard size={14} /> Shortcut
          </div>
          <div className="status-value">{prettyAccelerator(settings.shortcut)}</div>
          <span className="row-hint" style={{ margin: 0 }}>
            {settings.mode === "toggle" ? "Press to start, press again to stop" : "Hold to talk, release to insert"}
          </span>
          <div className="status-foot">
            <span />
            <button className="btn btn-sm" onClick={() => go("dictation")}>
              Change
            </button>
          </div>
        </section>
        <section className="status-card" aria-label="Language">
          <div className="status-label">
            <Languages size={14} /> Language
          </div>
          <div className="status-value">{settings.language === "auto" ? "Automatic (detect)" : langLabel(settings.language)}</div>
          <span className="row-hint" style={{ margin: 0 }}>
            {Object.keys(settings.languageModels ?? {}).length > 0 ? `${Object.keys(settings.languageModels ?? {}).length} with their own model` : "One model for every language"}
          </span>
          <div className="status-foot">
            <span />
            <button className="btn btn-sm" onClick={() => go("languages")}>
              Change
            </button>
          </div>
        </section>
        <section className="status-card" aria-label="Microphone">
          <div className="status-label">
            <Mic size={14} /> Microphone
          </div>
          <MicTest micName={mic} />
          <div className="status-foot">
            <span />
            <button className="btn btn-sm" onClick={() => go("dictation")}>
              Change
            </button>
          </div>
        </section>
      </div>
      <p className="page-desc">
        {settings.mode === "toggle" ? "Press" : "Hold"} {prettyAccelerator(settings.shortcut)}, speak{settings.mode === "toggle" ? ", press it again" : ""}, and the text lands in the app you were using.
      </p>
    </>
  );
}
