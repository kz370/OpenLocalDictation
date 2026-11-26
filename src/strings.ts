// English only. Keys keep the dotted form the shared overlay code uses.

const EN: Record<string, string> = {
  "app.automatic": "Automatic",
  "app.copied": "Copied",
  "app.copy": "Copy",
  "app.search": "Search",
  "voice.cancel": "Cancel",
  "voice.level": "Input level",
  "overlay.auto": "Auto",
  "overlay.cancelled": "Cancelled",
  "overlay.copyHint": "Copy reviewed text to the clipboard",
  "overlay.correcting": "Working…",
  "overlay.empty": "No speech detected",
  "overlay.error": "Dictation failed",
  "overlay.hint": "Release the keys to insert · Esc cancels",
  "overlay.hintReview": "Release the keys to review · Esc cancels",
  "overlay.hintToggle": "Press the shortcut again to insert · Esc cancels",
  "overlay.hintToggleReview": "Press the shortcut again to review · Esc cancels",
  "overlay.idle": "Ready",
  "overlay.settings": "Open settings",
  "overlay.close": "Close the dictation window",
  "overlay.idleHint": "Press {{keys}} or click the mic · nothing is heard until then",
  "overlay.micStart": "Start dictation",
  "overlay.middleClose": "middle-click to close",
  "overlay.micStop": "Stop and insert",
  "overlay.pin": "Pin: keep on screen, always on top",
  "overlay.unpin": "Unpin: hide when not dictating",
  "overlay.makeMini": "Small overlay",
  "overlay.makeFull": "Full overlay",
  "overlay.insert": "Insert",
  "overlay.insertHint": "Insert the text (Ctrl+Enter)",
  "overlay.reviewKeys": "Ctrl+Enter insert · Ctrl+Shift+Enter retry · Esc cancel",
  "overlay.inserted": "Inserted",
  "overlay.language": "Dictation language",
  "overlay.listeningLabel": "Listening",
  "overlay.restart": "Start over",
  "overlay.retry": "Retry",
  "overlay.retryHint": "Throw this away and dictate again (Ctrl+Shift+Enter)",
  "overlay.review": "Review — edit if needed",
  "overlay.reviewNow": "Review",
  "overlay.reviewNowHint": "Stop listening and edit the text before inserting it",
  "overlay.insertNowHint": "Stop listening and insert the text now",
  "overlay.reviewPending": "Finalizing dictation; you can edit now and insert when ready.",
  "overlay.searchLanguages": "Search languages…",
  "overlay.speakNow": "Start speaking…",
  "overlay.targetPrefix": "Insert into:",
  "overlay.targetUnknown": "Insert target unavailable · Copy text instead",
  "overlay.targetWindow": "Insert into: {{app}}",
  "overlay.transcribing": "Transcribing…",
  "settings.shortcuts.record": "Press the new shortcut…",
  "settings.shortcuts.reset": "Reset",
};

/** What went wrong and how to fix it, by backend error code. */
const ERRORS: Record<string, string> = {
  stt_unavailable: "No speech model is ready. Open Settings and download Whisper tiny, base or small.",
  audio: "The microphone could not be used. Check it in Windows Sound settings, or pick another one in Settings.",
  download: "The download failed. Check your internet connection and try again.",
  invalid: "That input was not valid.",
  io: "A file could not be read or written. Check disk space and folder permissions.",
  cancelled: "Cancelled.",
  other: "Something went wrong.",
};

export function t(key: string, vars?: Record<string, string | number>): string {
  let s = EN[key] ?? key;
  for (const [k, v] of Object.entries(vars ?? {})) s = s.replace(`{{${k}}}`, String(v));
  return s;
}

export function errorMessage(code: string, _detail?: string): string {
  return ERRORS[code] ?? ERRORS.other;
}

export function formatBytes(bytes: number): string {
  if (!bytes) return "0 MB";
  const gb = bytes / 1024 ** 3;
  if (gb >= 1) return `${gb.toFixed(1)} GB`;
  return `${Math.max(1, Math.round(bytes / 1024 ** 2))} MB`;
}
