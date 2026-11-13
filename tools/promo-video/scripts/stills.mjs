// Renders review stills in one bundle: `node scripts/stills.mjs 120 228 400` → out/s-0120.png …
import { bundle } from "@remotion/bundler";
import { renderStill, selectComposition } from "@remotion/renderer";
import path from "node:path";

const frames = process.argv.slice(2).map(Number);
const serveUrl = await bundle({ entryPoint: path.resolve("src/index.ts") });
const composition = await selectComposition({ serveUrl, id: "Promo" });
for (const frame of frames) {
  const output = `out/s-${String(frame).padStart(4, "0")}.png`;
  await renderStill({ serveUrl, composition, frame, output });
  console.log(output);
}
