import { createRoot } from "react-dom/client";

if (import.meta.env.DEV && location.search.includes("mock")) {
  await import("../dev/mock");
}
import "./pill.css";
import { Pill } from "./Pill";
import { listen } from "@tauri-apps/api/event";
import { applyAccent } from "../lib/theme";

applyAccent();
// The accent colour was changed in settings.
listen("accent-changed", () => applyAccent()).catch(() => {});

// Never show the browser context menu on the pill.
document.addEventListener("contextmenu", (e) => e.preventDefault());

createRoot(document.getElementById("pill-root")!).render(<Pill />);
