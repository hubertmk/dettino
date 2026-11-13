import { describe, expect, it } from "vitest";
import { expiry, expiryWarning } from "../lib/expiry";

const at = (s: string) => new Date(`${s}T15:30`);

describe("test build end date", () => {
  it("is absent for builds without one", () => {
    expect(expiry(null)).toBeNull();
    expect(expiry("soon")).toBeNull();
  });

  it("counts calendar days, whatever the time of day", () => {
    expect(expiry("2026-12-29", at("2026-10-30"))).toEqual({ date: "29 December 2026", days: 60, warn: false });
    expect(expiry("2026-12-29", at("2026-12-22"))?.warn).toBe(true);
    expect(expiry("2026-12-29", at("2026-12-21"))?.warn).toBe(false);
    expect(expiry("2026-12-29", at("2026-12-29"))?.days).toBe(0);
  });

  it("says how long it still works", () => {
    expect(expiryWarning(expiry("2026-12-29", at("2026-12-26"))!)).toBe("This test version works for 3 more days, until 29 December 2026.");
    expect(expiryWarning(expiry("2026-12-29", at("2026-12-28"))!)).toBe("This test version stops working after tomorrow.");
    expect(expiryWarning(expiry("2026-12-29", at("2026-12-29"))!)).toBe("This test version stops working after today.");
  });
});
