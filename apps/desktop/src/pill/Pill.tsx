// Picks the pill style from the state the Rust side emits and mounts that renderer.
// Each renderer keeps its own feeds; switching styles unmounts one and mounts the other,
// and the Rust side resizes the window to match (pill.rs `win_size`).
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ClassicPill } from "./Classic";
import { LinePill } from "./Line";
import type { PillState, PillStyle } from "./state";

export function Pill() {
  const [style, setStyle] = useState<PillStyle | null>(null);
  useEffect(() => {
    const pick = (s: PillState) => setStyle(s.style ?? "line");
    invoke<PillState>("pill_ready").then(pick).catch(() => setStyle("line"));
    const un = listen<PillState>("pill", (e) => pick(e.payload));
    return () => {
      un.then((u) => u());
    };
  }, []);
  if (!style) return null;
  return style === "line" ? <LinePill key="line" /> : <ClassicPill key="classic" />;
}
