import { createContext, useCallback, useContext, useEffect, useRef, useState } from "react";
import { api, setPlatform, Settings, Snapshot } from "./api";

interface Store {
  snap: Snapshot;
  /** Apply a settings change now (saved and applied in the background). */
  update: (fn: (s: Settings) => void) => void;
  setSnap: (s: Snapshot) => void;
  refresh: () => Promise<void>;
  toast: (msg: string) => void;
  /** Time of the last successful auto-save (0 = none yet). */
  savedAt: number;
}

const Ctx = createContext<Store | null>(null);

export function useStore() {
  const s = useContext(Ctx);
  if (!s) throw new Error("store missing");
  return s;
}

export function StoreProvider(p: { initial: Snapshot; toast: (m: string) => void; children: React.ReactNode }) {
  setPlatform(p.initial.platform);
  const [snap, setSnapState] = useState(p.initial);
  const [savedAt, setSavedAt] = useState(0);
  const pending = useRef<Settings | null>(null);
  const timer = useRef<number | undefined>(undefined);
  // The latest settings, readable outside React's render cycle.
  const latest = useRef<Settings>(p.initial.settings);
  const setSnap = useCallback((s: Snapshot) => {
    latest.current = s.settings;
    setSnapState(s);
  }, []);

  const flush = useCallback(async () => {
    const s = pending.current;
    if (!s) return;
    pending.current = null;
    try {
      const next = await api.saveSettings(s);
      // Keep edits made while this save was in flight.
      setSnap({ ...next, settings: pending.current ?? next.settings });
      setSavedAt(Date.now());
    } catch (e) {
      p.toast(String(e));
      api.getState().then(setSnap).catch(() => {});
    }
  }, [setSnap]);

  // The change is computed once, outside the state updater: React may run updaters
  // more than once, and a splice or push applied twice would remove or add two.
  const update = useCallback((fn: (s: Settings) => void) => {
    const next: Settings = structuredClone(pending.current ?? latest.current);
    fn(next);
    pending.current = next;
    latest.current = next;
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(flush, 350);
    setSnapState((cur) => ({ ...cur, settings: next }));
  }, [flush]);

  // Re-read from the backend (window focus, a change made by the tray or the pill).
  // An edit that is still waiting to be saved stays in place.
  const refresh = useCallback(async () => {
    try {
      const next = await api.getState();
      setSnap({ ...next, settings: pending.current ?? next.settings });
    } catch {
      /* the window may be closing */
    }
  }, [setSnap]);

  // An edit still waiting when the window goes away is saved, not dropped.
  useEffect(
    () => () => {
      window.clearTimeout(timer.current);
      if (pending.current) void flush();
    },
    [flush],
  );

  return <Ctx.Provider value={{ snap, update, setSnap, refresh, toast: p.toast, savedAt }}>{p.children}</Ctx.Provider>;
}
