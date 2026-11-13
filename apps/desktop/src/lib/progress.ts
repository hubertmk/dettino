// Download / install / pull progress, keyed by id, from the `model:progress` event.
import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { DownloadProgress } from "./api";

export function useModelProgress() {
  const [prog, setProg] = useState<Record<string, DownloadProgress>>({});
  useEffect(() => {
    const un = listen<DownloadProgress>("model:progress", (e) => setProg((p) => ({ ...p, [e.payload.id]: e.payload })));
    return () => {
      un.then((u) => u());
    };
  }, []);
  return prog;
}
