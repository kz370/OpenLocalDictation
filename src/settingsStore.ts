import { create } from "zustand";
import { ipc, on, toAppError } from "./ipc";
import type { AppErrorPayload, Settings } from "./types";

interface SettingsState {
  settings: Settings | null;
  error: AppErrorPayload | null;
  /** When the last change was saved (ms since epoch), for the "Saved" chip. */
  savedAt: number;
  load: () => Promise<Settings>;
  /** Applies `mutate` to a copy of the current settings and persists it. */
  update: (mutate: (draft: Settings) => void) => Promise<void>;
  subscribe: () => Promise<() => void>;
}

export const useSettings = create<SettingsState>((set, get) => ({
  settings: null,
  error: null,
  savedAt: 0,
  load: async () => {
    const s = await ipc.getSettings();
    set({ settings: s });
    applyTheme(s.theme);
    return s;
  },
  update: async (mutate) => {
    const current = get().settings;
    if (!current) return;
    const draft: Settings = structuredClone(current);
    mutate(draft);
    set({ settings: draft }); // optimistic
    applyTheme(draft.theme);
    try {
      const saved = await ipc.saveSettings(draft);
      set({ settings: saved, error: null, savedAt: Date.now() });
      applyTheme(saved.theme);
    } catch (e) {
      set({ settings: current, error: toAppError(e) });
      applyTheme(current.theme);
    }
  },
  subscribe: () =>
    on<Settings>("settings://changed", (s) => {
      set({ settings: s });
      applyTheme(s.theme);
    }),
}));

let mediaListener: ((e: MediaQueryListEvent) => void) | null = null;

/** "system" follows the OS and keeps following it while the app runs. */
export function applyTheme(theme: Settings["theme"]) {
  const media = window.matchMedia("(prefers-color-scheme: dark)");
  if (mediaListener) media.removeEventListener("change", mediaListener);
  mediaListener = null;
  const set = (dark: boolean) => (document.documentElement.dataset.theme = dark ? "dark" : "light");
  if (theme === "system") {
    set(media.matches);
    mediaListener = (e) => set(e.matches);
    media.addEventListener("change", mediaListener);
  } else {
    set(theme === "dark");
  }
}
