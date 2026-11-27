// Typed wrappers around Tauri commands and events.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AppErrorPayload, AppInfo, AudioDevice, GpuStatus, LoadedModel, ModelEntry, Settings, TaskbarStatus } from "./types";

export function isAppError(e: unknown): e is AppErrorPayload {
  return typeof e === "object" && e !== null && "code" in e && "detail" in e;
}

export function toAppError(e: unknown): AppErrorPayload {
  if (isAppError(e)) return e;
  return { code: "other", detail: e instanceof Error ? e.message : String(e) };
}

export const ipc = {
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  shortcutErrors: () => invoke<string[]>("shortcut_errors"),
  shortcutsCapture: (capturing: boolean, reviewShortcuts?: boolean) => invoke<void>("shortcuts_capture", { capturing, reviewShortcuts: reviewShortcuts ?? false }),
  appInfo: () => invoke<AppInfo>("app_info"),
  openLogs: () => invoke<void>("open_logs"),
  audioInputs: () => invoke<AudioDevice[]>("audio_inputs"),
  micTestStart: () => invoke<void>("mic_test_start"),
  micTestStop: () => invoke<void>("mic_test_stop"),

  sttLoaded: () => invoke<LoadedModel[]>("stt_loaded"),
  sttLoad: (id: string) => invoke<void>("stt_load", { id }),
  sttUnload: (id: string) => invoke<void>("stt_unload", { id }),
  modelsAddOwn: (dir: string) => invoke<ModelEntry>("models_add_own", { dir }),
  modelsList: () => invoke<ModelEntry[]>("models_list"),
  modelsDownload: (ids: string[]) => invoke<void>("models_download", { ids }),
  modelsCancel: (id: string) => invoke<void>("models_cancel", { id }),
  modelsSetDir: (dir: string | null) => invoke<string>("models_set_dir", { dir }),

  gpuStatus: () => invoke<GpuStatus>("gpu_status"),
  gpuSetEnabled: (enabled: boolean) => invoke<void>("gpu_set_enabled", { enabled }),
  gpuInstall: () => invoke<void>("gpu_install"),
  gpuCancel: () => invoke<void>("gpu_cancel"),
  settingsMinimize: () => invoke<void>("settings_minimize"),
  settingsToggleMaximize: () => invoke<boolean>("settings_toggle_maximize"),
  settingsIsMaximized: () => invoke<boolean>("settings_is_maximized"),
  settingsClose: () => invoke<void>("settings_close"),
  appRestart: () => invoke<void>("app_restart"),

  dictationCancel: () => invoke<void>("dictation_cancel"),
  dictationInsertNow: (text: string) => invoke<void>("dictation_insert_now", { text }),
  dictationConfirm: (text: string) => invoke<void>("dictation_confirm", { text }),
  dictationRetry: () => invoke<void>("dictation_retry"),
  dictationSetLanguage: (language: string) => invoke<void>("dictation_set_language", { language }),
  dictationResetOverlayPosition: () => invoke<void>("dictation_reset_overlay_position"),
  overlaySetOrbShift: (shift: number) => invoke<void>("overlay_set_orb_shift", { shift }),
  dictationToggle: () => invoke<void>("dictation_toggle"),
  settingsOpen: () => invoke<void>("settings_open"),
  overlaySetPinned: (pinned: boolean) => invoke<void>("overlay_set_pinned", { pinned }),
  overlayResize: (width: number, commit: boolean) => invoke<void>("overlay_resize", { width, commit }),
  overlaySetMini: (mini: boolean) => invoke<void>("overlay_set_mini", { mini }),
  overlayFitWidth: (width: number) => invoke<void>("overlay_fit_width", { width }),
  dictationOverlayCard: (shapes: [number, number, number, number][] | null) => invoke<void>("dictation_overlay_card", { shapes }),
  dictationOverlayBubble: (bubble: [number, number, number, number] | null) => invoke<void>("dictation_overlay_bubble", { bubble }),
  dictationOverlayMenu: (menu: [number, number, number, number] | null) => invoke<void>("dictation_overlay_menu", { menu }),
  overlayHideIdle: () => invoke<void>("overlay_hide_idle"),
  taskbarSetLanguages: (items: { code: string; label: string }[]) => invoke<void>("taskbar_set_languages", { items }),
  taskbarWidgetStatus: () => invoke<TaskbarStatus>("taskbar_widget_status"),
  taskbarWidgetResetPosition: () => invoke<void>("taskbar_widget_reset_position"),
};

/** Subscribes to a backend event; resolves to the unsubscribe function. */
export function on<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(event, (e) => handler(e.payload));
}
