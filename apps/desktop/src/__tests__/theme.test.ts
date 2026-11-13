import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { ACCENTS, clampAccent, contrast } from "../lib/theme";

const rgb = (h: string) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16)) as [number, number, number];

describe("OS accent clamp", () => {
  it("keeps an accent that already passes", () => {
    expect(clampAccent("#0067C0", "light")).toBe("#0067c0");
  });
  it("darkens Windows Gold until white text and white ground reach 4.5:1", () => {
    const v = clampAccent("#FFB900", "light")!;
    expect(contrast(rgb(v), [255, 255, 255])).toBeGreaterThanOrEqual(4.5);
  });
  it("lightens a dark accent for the dark theme", () => {
    const v = clampAccent("#0067C0", "dark")!;
    expect(contrast(rgb(v), [28, 28, 30])).toBeGreaterThanOrEqual(4.5);
  });
  it.each(ACCENTS)("$name passes AA in both themes, with the values in tokens.css", (a) => {
    // light: white text on the accent, the accent as text on white
    expect(contrast(rgb(a.light), [255, 255, 255])).toBeGreaterThanOrEqual(4.5);
    // dark: the accent as text on the dark surface, dark text on the accent
    expect(contrast(rgb(a.dark), [28, 28, 30])).toBeGreaterThanOrEqual(4.5);
    expect(contrast(rgb(a.darkText), rgb(a.dark))).toBeGreaterThanOrEqual(4.5);
  });
  it("lists every palette that tokens.css defines", () => {
    const css = readFileSync(resolve(__dirname, "../styles/tokens.css"), "utf8");
    for (const a of ACCENTS) {
      const block = a.id === "cobalt" ? css.slice(0, css.indexOf("[data-accent")) : css.match(new RegExp(`data-accent="${a.id}"\\][^}]*`))?.[0] ?? "";
      expect(block, a.id).toContain(a.light);
      expect(block, a.id).toContain(a.dark);
      expect(block, a.id).toContain(a.darkText);
    }
  });
  it("rejects malformed input", () => {
    expect(clampAccent("blue", "light")).toBeNull();
  });
});
