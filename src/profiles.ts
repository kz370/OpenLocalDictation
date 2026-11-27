import { useEffect, useState } from "react";
import { ipc } from "./ipc";
import { useSettings } from "./settingsStore";

/** How the app icon is recolored for a profile (src-tauri/src/profile.rs `TrayTint`). */
export interface Tint {
  hue: number;
  sat: number;
  light: number;
}

/** One color profile, as `color_profiles` sends it. */
export interface ColorProfile {
  id: string;
  label: string;
  accentLight: string;
  inkLight: string;
  accentDark: string;
  inkDark: string;
  tint: Tint | null;
}

const ICON = "/app-icon.png";
let loading: Promise<ColorProfile[]> | null = null;

/**
 * Loads the profiles once and puts each one's colors into a style sheet as
 * `--p-*` variables under `data-profile` (tokens.css derives the accent tokens
 * from them), so adding a profile is one row in profile.rs.
 */
export function loadProfiles(): Promise<ColorProfile[]> {
  loading ??= ipc.colorProfiles().then((list) => {
    const style = document.createElement("style");
    style.id = "profile-vars";
    style.textContent = list
      .map((p) => `:root[data-profile="${p.id}"]{--p-light:${p.accentLight};--p-ink-light:${p.inkLight};--p-dark:${p.accentDark};--p-ink-dark:${p.inkDark};}`)
      .join("\n");
    document.head.appendChild(style);
    return list;
  });
  return loading;
}

export function useProfiles(): ColorProfile[] {
  const [list, setList] = useState<ColorProfile[]>([]);
  useEffect(() => {
    let off = false;
    loadProfiles()
      .then((l) => !off && setList(l))
      .catch(() => {});
    return () => {
      off = true;
    };
  }, []);
  return list;
}

// Same math as `profile::tint_icon` in Rust (the tray icon): hue turned,
// saturation and lightness scaled, white and greys kept.
function rgbToHsl(r: number, g: number, b: number): [number, number, number] {
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  const d = max - min;
  if (d === 0) return [0, 0, l];
  const s = d / (1 - Math.abs(2 * l - 1));
  const h = max === r ? (((g - b) / d) % 6 + 6) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  return [h * 60, s, l];
}

function hslToRgb(h: number, s: number, l: number): [number, number, number] {
  const c = (1 - Math.abs(2 * l - 1)) * s;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = l - c / 2;
  const sector = Math.floor((((h % 360) + 360) % 360) / 60);
  const [r, g, b] = [[c, x, 0], [x, c, 0], [0, c, x], [0, x, c], [x, 0, c], [c, 0, x]][sector];
  return [r + m, g + m, b + m];
}

const tinted = new Map<string, Promise<string>>();

/** The app icon recolored for `tint`, as a data URL. */
export function tintedIcon(tint: Tint): Promise<string> {
  const key = `${tint.hue}/${tint.sat}/${tint.light}`;
  let hit = tinted.get(key);
  if (!hit) {
    hit = new Promise<string>((resolve, reject) => {
      const img = new Image();
      img.onload = () => {
        const canvas = document.createElement("canvas");
        canvas.width = img.naturalWidth;
        canvas.height = img.naturalHeight;
        const ctx = canvas.getContext("2d");
        if (!ctx) return reject(new Error("no canvas"));
        ctx.drawImage(img, 0, 0);
        const data = ctx.getImageData(0, 0, canvas.width, canvas.height);
        const px = data.data;
        for (let i = 0; i < px.length; i += 4) {
          const [, s, l] = rgbToHsl(px[i] / 255, px[i + 1] / 255, px[i + 2] / 255);
          if (s < 0.12) continue;
          const [r, g, b] = hslToRgb(tint.hue, Math.min(1, s * tint.sat), Math.min(1, l * tint.light));
          px[i] = Math.round(r * 255);
          px[i + 1] = Math.round(g * 255);
          px[i + 2] = Math.round(b * 255);
        }
        ctx.putImageData(data, 0, 0);
        resolve(canvas.toDataURL("image/png"));
      };
      img.onerror = () => reject(new Error("icon did not load"));
      img.src = ICON;
    });
    tinted.set(key, hit);
  }
  return hit;
}

/** The app icon in the current color profile (the original until it is ready). */
export function useAppIcon(): string {
  const id = useSettings((s) => s.settings?.colorProfile ?? "green");
  const [src, setSrc] = useState(ICON);
  useEffect(() => {
    let off = false;
    loadProfiles()
      .then((list) => {
        const tint = list.find((p) => p.id === id)?.tint;
        if (!tint) return ICON;
        return tintedIcon(tint);
      })
      .then((url) => !off && setSrc(url))
      .catch(() => !off && setSrc(ICON));
    return () => {
      off = true;
    };
  }, [id]);
  return src;
}
