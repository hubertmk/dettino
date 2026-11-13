import { describe, expect, it } from "vitest";
import type { Readiness, Stage } from "../lib/api";
import { autostartProblem, locationWarning, setupRows, startupFailure, startupLabel, startupSettled, statusWord } from "../lib/readiness";

const st = (state: "not_needed" | "missing" | "warming" | "ready"): Stage => ({ state });
const failed = (error: string): Stage => ({ state: "failed", error });
const r = (speech: Stage, ollama: Stage, cleanup: Stage): Readiness => ({
  speech,
  ollama,
  cleanup,
  all: [speech, ollama, cleanup].every((s) => s.state === "ready" || s.state === "not_needed"),
});

describe("startup words", () => {
  it("names the first stage still loading, in order", () => {
    expect(startupLabel(r(st("warming"), st("warming"), st("warming")))).toBe("Loading the speech model…");
    expect(startupLabel(r(st("ready"), st("warming"), st("warming")))).toBe("Starting Ollama…");
    expect(startupLabel(r(st("ready"), st("ready"), st("warming")))).toBe("Loading the cleanup model…");
    expect(startupLabel(r(st("ready"), st("ready"), st("ready")))).toBeNull();
  });
  it("is silent about the cleanup model while Ollama is still starting", () => {
    expect(startupFailure(r(st("ready"), st("warming"), st("missing")))).toBeNull();
    expect(startupSettled(r(st("ready"), st("warming"), st("missing")))).toBe(false);
  });
  it("settles on success or on the first failure", () => {
    expect(startupSettled(r(st("ready"), st("ready"), st("ready")))).toBe(true);
    const timeout = r(st("ready"), st("ready"), failed("The cleanup model did not answer within 90 seconds."));
    expect(startupSettled(timeout)).toBe(true);
    expect(startupFailure(timeout)).toContain("didn't answer");
    expect(startupFailure(r(st("ready"), failed("not answering"), st("warming")))).toBe("Ollama didn't start: not answering");
    expect(startupFailure(r(st("ready"), st("missing"), st("warming")))).toBe("Ollama is not installed.");
  });
  it("counts Cloud mode as ready without Ollama", () => {
    const fast = r(st("ready"), st("not_needed"), st("ready"));
    expect(fast.all).toBe(true);
    expect(statusWord(fast, false, "fast")).toBe("Ready · Cloud");
    expect(statusWord(r(st("ready"), st("ready"), st("warming")), true, "private")).toBe("Starting up…");
    expect(statusWord(r(st("ready"), failed("x"), st("warming")), true, "private")).toBe("Needs attention");
    expect(statusWord(r(st("ready"), st("ready"), st("ready")), true, "private")).toBe("Ready · Private");
  });
});

describe("Home rows", () => {
  it("points at what is missing on the cleanup side", () => {
    expect(setupRows(r(st("ready"), st("missing"), st("missing")))).toEqual([{ text: "Ollama is not installed.", label: "Run setup again", action: "setup" }]);
    expect(setupRows(r(st("ready"), failed("port taken"), st("warming")))[0]).toMatchObject({ label: "Start Ollama", action: "start_ollama" });
    expect(setupRows(r(st("ready"), st("ready"), st("missing")))).toEqual([{ text: "The cleanup model isn't downloaded.", label: "Run setup again", action: "setup" }]);
    expect(setupRows(r(st("ready"), st("not_needed"), st("missing")))[0].action).toBe("providers");
  });
  it("hides the cleanup row while Ollama is starting and shows nothing when all is well", () => {
    expect(setupRows(r(st("ready"), st("warming"), st("missing")))).toEqual([]);
    expect(setupRows(r(st("ready"), st("ready"), st("ready")))).toEqual([]);
    expect(setupRows(r(st("warming"), st("ready"), st("warming")))).toEqual([]);
  });
});

describe("launch at login, as the OS sees it", () => {
  it("says nothing while the setting and the OS agree, or while it is off", () => {
    expect(autostartProblem({ wanted: true, active: true, detail: null }, "windows")).toBeNull();
    expect(autostartProblem({ wanted: false, active: false, detail: null }, "windows")).toBeNull();
    expect(autostartProblem({ wanted: false, active: false, detail: "switched off" }, "macos")).toBeNull();
  });
  it("names the switch Windows or macOS used to turn it off", () => {
    expect(autostartProblem({ wanted: true, active: false, detail: null }, "windows")).toBe(
      "Dettino won't start with Windows: it is switched off under Task Manager → Startup apps, or the entry is missing.",
    );
    expect(autostartProblem({ wanted: true, active: false, detail: "switched off under System Settings → General → Login Items" }, "macos")).toBe(
      "Dettino won't start with macOS: switched off under System Settings → General → Login Items.",
    );
    expect(autostartProblem({ wanted: true, active: false, detail: null }, "macos")).toBe("Dettino won't start with macOS: it is not registered as a login item.");
  });
});

describe("where the app runs from", () => {
  it("only warns on macOS outside the Applications folder", () => {
    expect(locationWarning("applications")).toBeNull();
    expect(locationWarning("translocated")).toContain("temporary copy");
    expect(locationWarning("elsewhere")).toContain("Applications folder");
  });
});
