// Cobalt palette from site/index.html (the default `:root` tokens) and the app's tokens.css.
import { loadFont as loadGeist } from "@remotion/google-fonts/Geist";
import { loadFont as loadGeistMono } from "@remotion/google-fonts/GeistMono";

export const C = {
  desk: "#EFEFF2",
  sheet: "#FFFFFF",
  ink: "#1B1B1F",
  muted: "#5F5F66",
  line: "#E4E4E9",
  face: "#D9E2FA",
  accent: "#2F5FDB",
  ok: "#17784B",
  edge: "rgba(0,0,0,.14)",
  shade: "rgba(0,0,0,.3)",
} as const;

export const FONT = loadGeist("normal", { weights: ["400", "500", "600", "700"], subsets: ["latin"] }).fontFamily;
export const MONO = loadGeistMono("normal", { weights: ["500"], subsets: ["latin"] }).fontFamily;

export const FPS = 30;
/** Seconds to frames. */
export const s = (sec: number) => Math.round(sec * FPS);
