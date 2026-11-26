export interface AppErrorPayload {
  code: string;
  detail: string;
}

export interface Settings {
  model: string;
  modelsDir: string;
  device: "gpu" | "cpu";
  language: string;
  /** Language codes in the overlay menu; empty = all. */
  shownLanguages: string[];
  microphone: string | null;
  shortcut: string;
  mode: "hold" | "toggle";
  insertMethod: "type" | "paste";
  addTrailingSpace: boolean;
  reviewBeforeInsert: boolean;
  startWithOs: boolean;
  theme: "system" | "light" | "dark";
  vadThreshold: number;
  silenceMs: number;
  autoStopSilenceSecs: number;
  overlayX: number | null;
  overlayY: number | null;
  /** Overlay stays on screen, always on top; idle until shortcut or mic button. */
  overlayPinned: boolean;
  /** Small overlay layout. */
  overlayMini: boolean;
  /** Which overlay sizes are available; with "both" `overlayMini` is the current one. */
  /** Overlay background opacity, percent (30-100). */
  overlayOpacity: number;
  /** Settings (gear) button in the overlay header. */
  overlayShowSettings: boolean;
  /** Overlay drags sideways only, staying above the taskbar. */
  overlayHorizontalOnly: boolean;
  /** Overlay cannot be dragged. */
  overlayPositionLocked: boolean;
  overlayLayouts: "small" | "large" | "both";
  /** Small overlay width in logical px (320-800). */
  overlayMiniWidth: number;
  /** Bubble style: the mic circle's offset from the window center (logical px). */
  overlayOrbShift: number;
  /** No resize handle on the small overlay. */
  overlaySizeLocked: boolean;
  /** Small overlay look: one pill with the mic, or a mic bubble with the pill above it on hover. */
  overlayStyle: "pill" | "bubble";
  /** Mic button inside the Windows taskbar; the pill opens above it. */
  taskbarWidget: boolean;
  /** Where the taskbar button sits, logical px from the taskbar's left edge (null until first placed). */
  taskbarWidgetX: number | null;
}

/** Whether the taskbar button is on the taskbar and, if not, why. */
export interface TaskbarStatus {
  shown: boolean;
  reason: string | null;
}

export interface AppInfo {
  version: string;
  paths: { dataDir: string; logsDir: string };
  hasNvidia: boolean;
  gpuName: string | null;
  modelsDir: string;
  defaultModelsDir: string;
}

export interface AudioDevice {
  id: string;
  name: string;
  isDefault: boolean;
}

export interface ModelEntry {
  id: string;
  name: string;
  blurb: string;
  downloadBytes: number;
  installed: boolean;
  downloading: boolean;
  custom: boolean;
  path: string | null;
}

export interface DownloadProgress {
  modelId: string;
  state: "downloading" | "verifying" | "done" | "error" | "cancelled";
  downloadedBytes: number;
  totalBytes: number;
  error: string | null;
}

export interface GpuStatus {
  supported: boolean;
  gpuName: string | null;
  installed: boolean;
  sizeBytes: number;
  downloadBytes: number;
  active: boolean;
  restartRequired: boolean;
  reusable: boolean;
}

export interface GpuProgress {
  state: "downloading" | "extracting" | "done" | "error" | "cancelled";
  downloadedBytes: number;
  totalBytes: number;
  error: string | null;
}

export type VoiceEvent =
  | { type: "level"; value: number; bands: number[] }
  | { type: "partial"; text: string }
  | { type: "state"; session: number; state: string }
  | { type: "transcript"; text: string; audioMs: number; elapsedMs: number }
  | { type: "error"; code: string; detail: string };

export interface DictationStateEvent {
  state: "idle" | "listening" | "transcribing" | "correcting" | "reviewing" | "review" | "inserted" | "empty" | "cancelled" | "error";
  text?: string;
  targetApp?: string | null;
  result?: { raw: string; inserted: string };
  error?: AppErrorPayload;
}

export interface DictationTargetEvent {
  targetApp: string | null;
}
