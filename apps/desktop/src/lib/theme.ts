import { invoke } from "@tauri-apps/api/core";
import type { Accent } from "./api";

type RGB = [number, number, number];

function parse(hex: string): RGB | null {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return null;
  const n = parseInt(m[1], 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

const toHex = (c: RGB) => "#" + c.map((v) => Math.round(v).toString(16).padStart(2, "0")).join("");

function luminance([r, g, b]: RGB) {
  const f = (v: number) => {
    v /= 255;
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
}

export function contrast(a: RGB, b: RGB) {
  const [x, y] = [luminance(a), luminance(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
}

const mix = (c: RGB, to: RGB, t: number): RGB => [0, 1, 2].map((i) => c[i] + (to[i] - c[i]) * t) as RGB;

/** Darken (light theme) or lighten (dark theme) an accent until text on and around it reaches `min`:1. */
export function clampAccent(hex: string, theme: "light" | "dark", min = 4.5): string | null {
  const c = parse(hex);
  if (!c) return null;
  const ground: RGB = theme === "light" ? [255, 255, 255] : [28, 28, 30];
  const toward: RGB = theme === "light" ? [0, 0, 0] : [255, 255, 255];
  for (let t = 0; t <= 1; t += 0.02) {
    const v = mix(c, toward, t);
    if (contrast(v, ground) >= min) return toHex(v);
  }
  return toHex(toward);
}

/**
 * The named palettes. Keep in sync with the `data-accent` blocks in styles/tokens.css.
 * `light` is the accent as text and icons on the light theme; `fill` and `fillText`, when set,
 * are the button colour and the text on it (Seafoam is too light for white text).
 */
export const ACCENTS: { id: Exclude<Accent, "system">; name: string; light: string; dark: string; darkText: string; fill?: string; fillText?: string }[] = [
  { id: "surf", name: "Seafoam", light: "#0a7560", dark: "#2ec4a0", darkText: "#0b0d0c", fill: "#2ec4a0", fillText: "#0b0d0c" },
  { id: "cobalt", name: "Cobalt", light: "#2f5fdb", dark: "#6b9bff", darkText: "#0b1530" },
  { id: "ultramarine", name: "Ultramarine", light: "#2b3fd6", dark: "#8794ff", darkText: "#0b1033" },
  { id: "indigo", name: "Indigo", light: "#4f46e5", dark: "#9590ff", darkText: "#120f3a" },
  { id: "lettera", name: "Lettera", light: "#0b7285", dark: "#3bc3d4", darkText: "#03242a" },
  { id: "plum", name: "Plum", light: "#7b3fce", dark: "#b592ff", darkText: "#1d0b3a" },
  { id: "orchid", name: "Orchid", light: "#b0287c", dark: "#f07cc4", darkText: "#2e0620" },
  { id: "graphite", name: "Graphite", light: "#26262c", dark: "#e8e8ee", darkText: "#141416" },
  { id: "red", name: "Olivetti red", light: "#d2381f", dark: "#da5c47", darkText: "#1f0702" },
];

/** Fired on `window` once the accent tokens have been (re)applied, so canvas drawing can re-read them. */
export const ACCENT_APPLIED = "dettino:accent-applied";

const OS_VARS = ["--os-accent-light", "--os-accent-dark", "--os-accent-text-dark"];

/**
 * Apply the palette picked in Settings: `data-accent` selects it in tokens.css. For "System",
 * the OS accent (clamped to AA) overrides it; without an OS accent, Seafoam stays. Safe to call
 * again whenever the setting changes.
 */
export async function applyAccent() {
  const root = document.documentElement;
  try {
    const { palette, os } = await invoke<{ palette: Accent; os: string | null }>("accent");
    root.dataset.accent = palette;
    const light = os ? clampAccent(os, "light") : null;
    const dark = os ? clampAccent(os, "dark") : null;
    const values = [light, dark, dark ? toHex(mix(parse(dark)!, [0, 0, 0], 0.85)) : null];
    OS_VARS.forEach((v, i) => (values[i] ? root.style.setProperty(v, values[i]) : root.style.removeProperty(v)));
  } catch {
    // The Seafoam tokens stay in place.
  }
  window.dispatchEvent(new Event(ACCENT_APPLIED));
}
