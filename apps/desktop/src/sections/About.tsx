import { Coffee, Copy } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api } from "../lib/api";
import { useStore } from "../lib/store";
import { APP_NAME, BUY_ME_A_COFFEE_URL, osName } from "../lib/brand";
import { expiry } from "../lib/expiry";
import { Row } from "../components/ui";

const LICENSES = [
  ["Tauri", "MIT / Apache-2.0"],
  ["sherpa-onnx (via sherpa-rs)", "Apache-2.0 / MIT"],
  ["NVIDIA Parakeet TDT 0.6B", "CC-BY-4.0 — © NVIDIA"],
  ["OpenAI Whisper weights", "MIT"],
  ["Silero VAD", "MIT"],
  ["cpal, rusqlite, reqwest, tokio, windows-rs", "MIT / Apache-2.0"],
  ["React, Vite, lucide", "MIT / ISC"],
];

export function About({ rerunSetup }: { rerunSetup: () => void }) {
  const { snap, toast } = useStore();
  const b = snap.build;
  const ends = expiry(b.expires);
  const until = ends ? ` · test version, works until ${ends.date}` : "";
  const buildNote = (b.dirty ? " · built with uncommitted changes" : b.release ? "" : " · not a tagged release") + (b.profile === "debug" ? " · debug build" : "") + until;
  const copyBuild = async () => {
    await api.copyText(`${APP_NAME} ${b.label} · commit ${b.commit} (${b.commit_date}) · ${b.profile} build · ${snap.platform}${until}`);
    toast("Build info copied");
  };
  return (
    <div className="page">
      <div className="page-head">
        <h1>About {APP_NAME}</h1>
        <p>Version {b.label} · {osName(snap.platform)}</p>
      </div>
      <div className="card card-pad stack">
        <b>No accounts. No telemetry. No screenshots.</b>
        <span className="muted">
          Your voice and text go only to the providers you choose. Settings, history and recordings stay in {snap.data_dir}; keys stay in the system keychain.
        </span>
        <div><button className="btn small" onClick={rerunSetup}>Run setup again</button></div>
      </div>
      <div className="card">
        <Row title="Build" desc={`Commit ${b.commit} from ${b.commit_date}${buildNote}`}>
          <button className="btn small" onClick={copyBuild} title="Copy the version and build details for a bug report"><Copy /> Copy</button>
        </Row>
      </div>
      <div className="card">
        <Row title={`Enjoying ${APP_NAME}?`} desc="It is free and open source, with no accounts or subscriptions. A coffee pays for the signing certificates and test machines.">
          <button className="btn small" onClick={() => openUrl(BUY_ME_A_COFFEE_URL)} title="Opens buymeacoffee.com in your browser"><Coffee /> Buy me a coffee</button>
        </Row>
      </div>
      <div className="card">
        <div className="card-head"><h3>Open-source components</h3></div>
        {LICENSES.map(([n, l]) => (
          <div className="row" key={n}><span className="grow">{n}</span><span className="small muted">{l}</span></div>
        ))}
      </div>
    </div>
  );
}
