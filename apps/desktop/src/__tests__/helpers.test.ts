import { describe, expect, it } from "vitest";
import { chordLabel, keyName, MODS, setPlatform, vkFromEvent, formatBytes } from "../lib/api";
import { wordDiff } from "../sections/History";

describe("chords", () => {
  it("labels default chords", () => {
    expect(chordLabel({ mods: MODS.CTRL, key: 0x20 })).toBe("Ctrl + Space");
    expect(chordLabel({ mods: MODS.CTRL | MODS.SHIFT, key: 0x20 })).toBe("Ctrl + Shift + Space");
    expect(chordLabel({ mods: 0, key: 0xa3 })).toBe("Right Ctrl");
    expect(keyName(0x5a)).toBe("Z");
    expect(keyName(0x70)).toBe("F1");
  });
  it("uses the macOS key names on macOS", () => {
    setPlatform("macos");
    try {
      expect(chordLabel({ mods: MODS.CTRL, key: 0x20 })).toBe("Control + Space");
      expect(chordLabel({ mods: MODS.CTRL | MODS.ALT, key: 0x5a })).toBe("Control + Option + Z");
      expect(chordLabel({ mods: MODS.WIN, key: 0x56 })).toBe("Command + V");
      expect(chordLabel({ mods: 0, key: 0xa5 })).toBe("Right Option");
      expect(keyName(0xff)).toBe("Fn");
    } finally {
      setPlatform("windows");
    }
  });
  it("maps keyboard events to virtual keys independent of layout", () => {
    expect(vkFromEvent(new KeyboardEvent("keydown", { code: "Space" }))).toBe(0x20);
    expect(vkFromEvent(new KeyboardEvent("keydown", { code: "KeyZ", key: "y" }))).toBe(0x5a);
    expect(vkFromEvent(new KeyboardEvent("keydown", { code: "ControlRight" }))).toBe(0xa3);
    expect(vkFromEvent(new KeyboardEvent("keydown", { code: "F12" }))).toBe(0x7b);
  });
  it("formats sizes", () => {
    expect(formatBytes(482_468_385)).toBe("482 MB");
    expect(formatBytes(1_600_000_000)).toBe("1.6 GB");
  });
});

describe("wordDiff", () => {
  it("marks removed fillers and corrections", () => {
    const d = wordDiff("um let's meet at two actually three", "let's meet at three");
    const removed = d.filter((x) => x.t === "del").map((x) => x.s).join("");
    expect(removed).toContain("um");
    expect(removed).toContain("two");
    expect(d.filter((x) => x.t === "eq").map((x) => x.s).join("")).toContain("meet at");
  });
  it("is identity for equal text", () => {
    expect(wordDiff("a b", "a b")).toEqual([{ t: "eq", s: "a b" }]);
  });
});
