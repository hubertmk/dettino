// The one-click local setup: download the speech model, install or start Ollama,
// pull the cleanup model, one after another. It lives above the onboarding so it keeps
// going (and keeps its progress) while the user moves on to the next steps.
import { createContext, useCallback, useContext, useEffect, useRef, useState } from "react";
import { api, DownloadProgress, OllamaStatus, Readiness } from "./api";
import { useStore } from "./store";
import { useModelProgress } from "./progress";
import { PlanStep, planProgress, providerNeeds, setupPlan, StepKind, stepLabel } from "./setup";

const DEFAULT_BASE = "http://localhost:11434";

export interface SetupJob {
  /** Last Ollama probe; null until the first one answers (or when Ollama isn't needed). */
  ollama: OllamaStatus | null;
  refreshOllama: () => Promise<void>;
  /** What the current mode needs, with done flags (live). */
  plan: PlanStep[];
  /** The plan is known (Ollama was probed if it is needed). */
  checked: boolean;
  /** Nothing left to install. */
  ready: boolean;
  /** What is installed is also loaded and answering (the backend's readiness). */
  warm: boolean;
  readiness: Readiness;
  state: "idle" | "running" | "done" | "error";
  error: string | null;
  /** The step that failed or is running. */
  current: PlanStep | null;
  /** The steps the running job works through, in order (empty before it starts). */
  queue: PlanStep[];
  /** Progress over the plan as it was when the job started. */
  progress: { done: number; total: number; pct: number };
  label: string;
  freeMb: number | null;
  start: () => void;
  prog: Record<string, DownloadProgress>;
}

const Ctx = createContext<SetupJob | null>(null);

export function useSetup() {
  const s = useContext(Ctx);
  if (!s) throw new Error("setup job missing");
  return s;
}

export function SetupProvider({ children }: { children: React.ReactNode }) {
  const { snap, setSnap } = useStore();
  const prog = useModelProgress();
  const [ollama, setOllama] = useState<OllamaStatus | null>(null);
  const [freeMb, setFreeMb] = useState<number | null>(null);
  const [job, setJob] = useState<{ state: SetupJob["state"]; plan: PlanStep[]; finished: StepKind[]; current: StepKind | null; error: string | null }>({
    state: "idle",
    plan: [],
    finished: [],
    current: null,
    error: null,
  });
  const running = useRef(false);
  // The job awaits several commands; read the newest snapshot, not the one it started with.
  const snapRef = useRef(snap);
  snapRef.current = snap;

  const needs = providerNeeds(snap);
  const base = needs.ollama?.base_url ?? DEFAULT_BASE;

  const refreshOllama = useCallback(async () => {
    try {
      setOllama(await api.ollamaStatus(base));
    } catch {
      setOllama({ installed: false, reachable: false, models: [], version: null, can_install: false });
    }
  }, [base]);

  useEffect(() => {
    if (needs.ollama) refreshOllama();
  }, [!!needs.ollama, refreshOllama]);

  useEffect(() => {
    api.freeDiskMb().then(setFreeMb, () => setFreeMb(null));
  }, [job.state]);

  const start = useCallback(() => {
    if (running.current) return;
    running.current = true;
    (async () => {
      let st: OllamaStatus | null = null;
      if (providerNeeds(snapRef.current).ollama) {
        st = await api.ollamaStatus(base).catch(() => null);
        setOllama(st);
      }
      const plan = setupPlan(snapRef.current, st).filter((s) => !s.done);
      const finished: StepKind[] = [];
      setJob({ state: "running", plan, finished: [], current: null, error: null });
      try {
        for (const step of plan) {
          setJob((j) => ({ ...j, current: step.kind }));
          if (step.kind === "speech") {
            for (const id of step.ids) setSnap(await api.modelDownload(id));
          } else if (step.kind === "ollama") {
            let s = await api.ollamaStatus(base);
            if (!s.reachable) {
              if (s.installed) s = await api.ollamaLaunch(base);
              else if (s.can_install) s = await api.ollamaInstall(base);
              else throw "Ollama can't be installed from inside Dettino on this computer. Install it from the download page, then click Try again.";
            }
            setOllama(s);
          } else if (step.tag) {
            const s = await api.ollamaStatus(base);
            if (!s.models.includes(step.tag)) setSnap(await api.ollamaPull(step.tag, base));
            setOllama(await api.ollamaStatus(base));
          }
          finished.push(step.kind);
          setJob((j) => ({ ...j, finished: [...finished] }));
        }
        setJob((j) => ({ ...j, state: "done", current: null }));
      } catch (e) {
        setJob((j) => ({ ...j, state: "error", error: String(e) }));
      } finally {
        running.current = false;
      }
    })();
  }, [base, setSnap]);

  const plan = setupPlan(snap, ollama);
  const checked = !needs.ollama || ollama !== null;
  const current = job.plan.find((s) => s.kind === job.current) ?? null;
  const value: SetupJob = {
    ollama,
    refreshOllama,
    plan,
    checked,
    ready: checked && plan.every((s) => s.done),
    warm: snap.readiness.all,
    readiness: snap.readiness,
    state: job.state,
    error: job.error,
    current,
    queue: job.plan,
    progress: planProgress(job.plan, prog, job.finished),
    label: current ? stepLabel(current, prog) : "",
    freeMb,
    start,
    prog,
  };
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}
