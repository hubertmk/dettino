import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  AppWindow, BookA, Boxes, Check, Cpu, History as HistoryIcon, Home as HomeIcon, Keyboard, Search, SlidersHorizontal,
} from "lucide-react";
import { api, Snapshot, Suggestion } from "./lib/api";
import { StoreProvider, useStore } from "./lib/store";
import { SetupProvider } from "./lib/setupJob";
import { statusWord } from "./lib/readiness";
import { APP_NAME, BrandMark } from "./lib/brand";
import { useToast } from "./components/ui";
import { Onboarding } from "./onboarding/Onboarding";
import { Home } from "./sections/Home";
import { Providers } from "./sections/Providers";
import { Dictation } from "./sections/Dictation";
import { Privacy } from "./sections/Privacy";
import { Dictionary } from "./sections/Dictionary";
import { Snippets } from "./sections/Snippets";
import { AppRules } from "./sections/AppRules";
import { History } from "./sections/History";
import { Audio } from "./sections/Audio";
import { Advanced } from "./sections/Advanced";
import { About } from "./sections/About";

/** A page. Backend `navigate` events and in-app links address pages by these ids. */
export type SectionId =
  | "home" | "providers" | "dictation" | "privacy" | "dictionary" | "snippets" | "apps" | "history" | "audio" | "advanced" | "about";

const PAGES: Record<SectionId, { label: string; keywords: string }> = {
  home: { label: "Home", keywords: "status setup pause" },
  providers: { label: "Providers", keywords: "preset private cloud fast balanced openai anthropic perplexity ollama lm studio whisper parakeet model key api fallback hedge download gpu" },
  dictation: { label: "Shortcuts & cleanup", keywords: "shortcut hotkey ctrl space hands-free command cleanup light medium off instruction sounds overlay pill style classic line dot success animation flash check pasted" },
  audio: { label: "Microphone", keywords: "audio microphone device bluetooth pre-roll warm level test" },
  dictionary: { label: "Dictionary", keywords: "words names terms learn suggestions" },
  snippets: { label: "Snippets", keywords: "trigger expand variables date signature" },
  apps: { label: "App rules", keywords: "per app terminal denylist never dictate paste type delay banking" },
  history: { label: "History", keywords: "past dictations raw cleaned diff audio replay retry timing" },
  privacy: { label: "Context & privacy", keywords: "privacy context screen text caret selection local only history retention delete preview" },
  advanced: { label: "Advanced", keywords: "logs export import clipboard login hedge startup colour color accent red theme" },
  about: { label: "About", keywords: "version licenses privacy setup again" },
};

/** The sidebar: seven groups; a group with several pages shows them as tabs. */
const GROUPS: { id: string; label: string; icon: React.ComponentType; pages: SectionId[] }[] = [
  { id: "home", label: "Home", icon: HomeIcon, pages: ["home"] },
  { id: "hearing", label: "Hearing & cleanup", icon: Boxes, pages: ["providers"] },
  { id: "dictation", label: "Dictation", icon: Keyboard, pages: ["dictation", "audio"] },
  { id: "words", label: "Words", icon: BookA, pages: ["dictionary", "snippets"] },
  { id: "apps", label: "Apps", icon: AppWindow, pages: ["apps"] },
  { id: "history", label: "History", icon: HistoryIcon, pages: ["history"] },
  { id: "general", label: "General", icon: SlidersHorizontal, pages: ["privacy", "advanced", "about"] },
];

const groupOf = (page: SectionId) => GROUPS.find((g) => g.pages.includes(page)) ?? GROUPS[0];

export function App() {
  const [snap, setSnap] = useState<Snapshot | null>(null);
  const [toastNode, toast] = useToast();
  useEffect(() => {
    api.getState().then(setSnap);
  }, []);
  if (!snap) return null;
  return (
    <StoreProvider initial={snap} toast={toast}>
      <SetupProvider>
        <Shell />
      </SetupProvider>
      {toastNode}
    </StoreProvider>
  );
}

function Shell() {
  const { snap, refresh, savedAt } = useStore();
  const [section, setSection] = useState<SectionId>((new URLSearchParams(location.search).get("section") as SectionId) || "home");
  const [focusId, setFocusId] = useState<string | null>(new URLSearchParams(location.search).get("focus"));
  const [query, setQuery] = useState("");
  const [suggestions, setSuggestions] = useState<Suggestion[]>([]);
  const [onboarding, setOnboarding] = useState(!snap.settings.onboarding_done);
  const [showSaved, setShowSaved] = useState(false);

  useEffect(() => {
    const un = listen<string>("navigate", (e) => {
      const [sec, id] = e.payload.split(":");
      if (sec in PAGES) setSection(sec as SectionId);
      setFocusId(id ?? null);
    });
    const un2 = listen("dictionary:suggestions", () => api.suggestionsList().then(setSuggestions));
    const un3 = listen("settings:changed", () => refresh());
    // The models loaded, Ollama came up, a warm-up failed: the snapshot carries it.
    const un4 = listen("readiness", () => refresh());
    api.suggestionsList().then(setSuggestions);
    const onFocus = () => refresh();
    window.addEventListener("focus", onFocus);
    return () => {
      un.then((u) => u());
      un2.then((u) => u());
      un3.then((u) => u());
      un4.then((u) => u());
      window.removeEventListener("focus", onFocus);
    };
  }, []);

  useEffect(() => {
    if (!savedAt) return;
    setShowSaved(true);
    const t = setTimeout(() => setShowSaved(false), 1600);
    return () => clearTimeout(t);
  }, [savedAt]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return GROUPS.map((g) => ({ g, first: g.pages[0] }));
    return GROUPS.flatMap((g) => {
      const hit = g.pages.find((p) => g.label.toLowerCase().includes(q) || PAGES[p].label.toLowerCase().includes(q) || PAGES[p].keywords.includes(q));
      return hit ? [{ g, first: hit }] : [];
    });
  }, [query]);

  const ready = snap.settings.asr_chain.some((id) => snap.ready[id]);
  const paused = snap.settings.paused;
  const go = (p: SectionId) => {
    setSection(p);
    setFocusId(null);
  };

  if (onboarding) {
    return <Onboarding onDone={() => setOnboarding(false)} />;
  }

  const group = groupOf(section);
  return (
    <div className="shell">
      <nav className="sidebar" aria-label="Settings sections">
        <div className="brand">
          <BrandMark className="brand-mark" />
          {APP_NAME}
        </div>
        <div className="search">
          <Search />
          <input placeholder="Search settings" value={query} onChange={(e) => setQuery(e.target.value)} aria-label="Search settings" />
        </div>
        {filtered.map(({ g, first }) => {
          const Icon = g.icon;
          const active = g.id === group.id;
          return (
            <button key={g.id} className={`nav-item ${active ? "active" : ""}`} aria-current={active ? "page" : undefined} onClick={() => go(query ? first : g.pages[0])}>
              <Icon />
              {g.label}
              {g.id === "words" && suggestions.length > 0 && <span className="count" aria-label={`${suggestions.length} suggestions`}>{suggestions.length}</span>}
            </button>
          );
        })}
        <div className="spacer" />
        <div className="status" role="status">
          <Cpu size={14} />
          {paused ? "Paused" : !ready ? "Needs setup" : statusWord(snap.readiness, snap.fully_local, snap.settings.preset)}
        </div>
        {/* Which build is this? Readable on any screenshot; opens About, which has the details and a Copy button. */}
        <button className="version" onClick={() => go("about")} title={`About ${APP_NAME}`}>
          {APP_NAME} {snap.build.label}
        </button>
      </nav>
      <main className="main">
        {group.pages.length > 1 && (
          <div className="subtabs" role="tablist" aria-label={group.label}>
            {group.pages.map((p) => (
              <button key={p} role="tab" aria-selected={p === section} className={p === section ? "on" : ""} onClick={() => go(p)}>
                {PAGES[p].label}
              </button>
            ))}
          </div>
        )}
        {section === "home" && <Home go={go} rerunSetup={() => setOnboarding(true)} />}
        {section === "providers" && <Providers />}
        {section === "dictation" && <Dictation />}
        {section === "privacy" && <Privacy />}
        {section === "dictionary" && <Dictionary suggestions={suggestions} setSuggestions={setSuggestions} />}
        {section === "snippets" && <Snippets />}
        {section === "apps" && <AppRules />}
        {section === "history" && <History focusId={focusId} />}
        {section === "audio" && <Audio />}
        {section === "advanced" && <Advanced />}
        {section === "about" && <About rerunSetup={() => setOnboarding(true)} />}
        <div className={`saved ${showSaved ? "on" : ""}`} role="status" aria-live="polite">
          {showSaved && <><Check size={14} /> Saved</>}
        </div>
      </main>
    </div>
  );
}
