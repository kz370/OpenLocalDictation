import { useCallback, useEffect, useState } from "react";
import { ipc } from "../../ipc";
import { KNOWN_LANGUAGES } from "../../knownLanguages";
import type { LoadedModel, ModelEntry, Settings } from "../../types";
import type { SearchOption } from "../../components/SearchSelect";

export type Update = (m: (d: Settings) => void) => void;

export const languageOptions: SearchOption[] = [
  { value: "auto", label: "Automatic (detect)" },
  ...KNOWN_LANGUAGES.map((l) => ({ value: l.code, label: l.name === l.native ? l.name : `${l.name} · ${l.native}` })),
];

export const langLabel = (code: string) => {
  const l = KNOWN_LANGUAGES.find((k) => k.code === code);
  return l ? (l.name === l.native ? l.name : `${l.name} · ${l.native}`) : code;
};

/** The family name is already the section's, so the lists show only the size: "small", "large-v3 turbo". */
export const shortModel = (name: string | undefined) => (name ?? "").replace(/^whisper[\s-]+/i, "");

export const percent = (done: number, total: number) => (total > 0 ? Math.min(100, Math.round((done / total) * 100)) : 0);

/** "GPU" or "CPU" badge for the loaded model. */
export const deviceLabel = (m: LoadedModel) => (m.device === "gpu" ? "GPU (CUDA)" : m.gpuBlocked ? "CPU only" : "CPU");

/**
 * The model used for every language: the chosen one if present, else the
 * largest downloaded one, else the first model found in the folder. Same rule
 * as the backend.
 */
export function resolveActiveModel(models: ModelEntry[], settings: Settings): string | undefined {
  const installed = models.filter((m) => m.installed);
  const downloaded = installed.filter((m) => !m.custom);
  return installed.find((m) => m.id === settings.model)?.id ?? downloaded[downloaded.length - 1]?.id ?? installed[0]?.id;
}

/** The models in memory now (they load at start-up and at takes, so the list changes while dictating); polled while the page is open. */
export function useLoadedModels(): [LoadedModel[], () => void] {
  const [loaded, setLoaded] = useState<LoadedModel[]>([]);
  const poll = useCallback(() => void ipc.sttLoaded().then((l) => setLoaded(Array.isArray(l) ? l : [])).catch(() => undefined), []);
  useEffect(() => {
    poll();
    const t = window.setInterval(poll, 1500);
    return () => window.clearInterval(t);
  }, [poll]);
  return [loaded, poll];
}
