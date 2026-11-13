import { createRoot } from "react-dom/client";

if (import.meta.env.DEV && location.search.includes("mock")) {
  await import("./dev/mock");
}
import "./styles/app.css";
import { App } from "./App";
import { listen } from "@tauri-apps/api/event";
import { applyAccent } from "./lib/theme";

applyAccent();
// The accent colour was changed in settings.
listen("accent-changed", () => applyAccent()).catch(() => {});

createRoot(document.getElementById("root")!).render(<App />);
