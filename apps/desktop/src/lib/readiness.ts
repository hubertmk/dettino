// What the backend says is loaded and answering (the `readiness` event / snapshot
// field), turned into words for the setup card, the Home page and the sidebar.
// Pure helpers, unit-tested.
import { type AppLocation, type AutostartState, PRESET_LABEL, type Preset, type Readiness, type Stage } from "./api";
import { osName } from "./brand";

export const stageOk = (s: Stage) => s.state === "ready" || s.state === "not_needed";
export const stageFailed = (s: Stage): s is { state: "failed"; error: string } => s.state === "failed";

/** The line under the setup card while things load: the first stage still pending. */
export function startupLabel(r: Readiness): string | null {
  if (r.all) return null;
  if (r.speech.state === "warming") return "Loading the speech model…";
  if (r.ollama.state === "warming") return "Starting Ollama…";
  if (r.cleanup.state === "warming") return "Loading the cleanup model…";
  return null;
}

/** Why start-up cannot finish, or null while it is still going (or done). */
export function startupFailure(r: Readiness): string | null {
  if (r.speech.state === "missing") return "The speech model isn't downloaded.";
  if (stageFailed(r.speech)) return `The speech model didn't load: ${r.speech.error}`;
  if (r.ollama.state === "missing") return "Ollama is not installed.";
  if (stageFailed(r.ollama)) return `Ollama didn't start: ${r.ollama.error}`;
  // What the cleanup model does is unknown while Ollama is still starting.
  if (!stageOk(r.ollama)) return null;
  if (r.cleanup.state === "missing") return r.ollama.state === "ready" ? "The cleanup model isn't downloaded." : "No cleanup provider is set up.";
  if (stageFailed(r.cleanup)) return `The cleanup model didn't answer: ${r.cleanup.error}`;
  return null;
}

/** Start-up is over, one way or the other: everything runs, or something says why not. */
export function startupSettled(r: Readiness): boolean {
  return r.all || startupFailure(r) !== null;
}

export interface SetupRow {
  text: string;
  label: string;
  action: "providers" | "setup" | "start_ollama";
}

/** Home's "Finish setup" rows for the cleanup side; the speech side is covered by `snap.ready`. */
export function setupRows(r: Readiness): SetupRow[] {
  const rows: SetupRow[] = [];
  if (stageFailed(r.speech)) rows.push({ text: `The speech model didn't load: ${r.speech.error}`, label: "Providers", action: "providers" });
  if (r.ollama.state === "missing") rows.push({ text: "Ollama is not installed.", label: "Run setup again", action: "setup" });
  else if (stageFailed(r.ollama)) rows.push({ text: `Ollama didn't start: ${r.ollama.error}`, label: "Start Ollama", action: "start_ollama" });
  else if (stageOk(r.ollama)) {
    if (r.cleanup.state === "missing") {
      rows.push(
        r.ollama.state === "ready"
          ? { text: "The cleanup model isn't downloaded.", label: "Run setup again", action: "setup" }
          : { text: "No cleanup provider is ready — dictations are pasted as heard.", label: "Providers", action: "providers" },
      );
    } else if (stageFailed(r.cleanup)) {
      rows.push({ text: `The cleanup model didn't answer: ${r.cleanup.error}`, label: "Providers", action: "providers" });
    }
  }
  return rows;
}

/** The sidebar's one-word status once a transcription provider is ready. */
export function statusWord(r: Readiness, fullyLocal: boolean, preset: string): string {
  if (!r.all) return startupFailure(r) ? "Needs attention" : "Starting up…";
  return fullyLocal ? "Ready · Private" : `Ready · ${PRESET_LABEL[preset as Preset] ?? preset[0].toUpperCase() + preset.slice(1)}`;
}

/** The setting says "start at login" but the OS will not do it: why, in one line. Null when they agree. */
export function autostartProblem(a: AutostartState, platform: string): string | null {
  if (!a.wanted || a.active) return null;
  const os = osName(platform);
  if (a.detail) return `Dettino won't start with ${os}: ${a.detail.replace(/\.$/, "")}.`;
  if (platform === "windows") return `Dettino won't start with ${os}: it is switched off under Task Manager → Startup apps, or the entry is missing.`;
  return `Dettino won't start with ${os}: it is not registered as a login item.`;
}

/** macOS: the app runs from a place that loses permissions and launch at login by the next login. */
export function locationWarning(loc: AppLocation): string | null {
  switch (loc) {
    case "translocated":
      return "macOS is running Dettino from a temporary copy. Move it to the Applications folder, then open it from there, so permissions and Start at login stick.";
    case "elsewhere":
      return "Dettino is not in the Applications folder. Move it there and open it from there, so permissions and Start at login stick.";
    default:
      return null;
  }
}
