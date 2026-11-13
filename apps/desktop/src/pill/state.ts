// Shared between the two pill styles: the state the Rust side emits and a few helpers.
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/** Which look the pill window renders; mirrors `app_core::PillStyle`. */
export type PillStyle = "classic" | "line";
export type SuccessAnimation = "flash" | "check" | "check_word";

export interface PillAction {
  id: string;
  label: string;
}
export interface PillState {
  /** idle | starting | ready | listening | thinking | pasted | pasted_raw | held | nothing | cancelled | error | blocked | notice */
  kind: string;
  mode: string;
  hands_free: boolean;
  quiet: boolean;
  label: string | null;
  detail: string | null;
  action: PillAction | null;
  since_ms: number;
  seq: number;
  badge: string | null;
  badge_label: string | null;
  undo_until_ms: number | null;
  style: PillStyle;
  /** How a successful paste is shown (One line only). */
  success: SuccessAnimation;
  /** Hold-to-talk chord, e.g. "Ctrl + Space". */
  hint: string;
}

export const IDLE: PillState = {
  kind: "idle", mode: "dictation", hands_free: false, quiet: false, label: null, detail: null, action: null,
  since_ms: Date.now(), seq: 0, badge: null, badge_label: null, undo_until_ms: null,
  style: "line", success: "flash", hint: "Ctrl + Space",
};

export const act = (id: string) => invoke("pill_action", { id });

export function useNow(active: boolean, every = 250) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    if (!active) return;
    setNow(Date.now());
    const t = setInterval(() => setNow(Date.now()), every);
    return () => clearInterval(t);
  }, [active, every]);
  return now;
}

export function mmss(ms: number) {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}
