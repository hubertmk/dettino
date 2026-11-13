// A test build's end date (`BuildInfo.expires`, see expiry.rs), in words for the
// Home page and About. Pure helpers, unit-tested.

/** From this many days before the end, Home shows a warning instead of a quiet line. */
export const WARN_DAYS = 7;

export interface Expiry {
  /** "29 December 2026" */
  date: string;
  /** Days left after today: 0 on the last day. */
  days: number;
  warn: boolean;
}

export function expiry(expires: string | null, now = new Date()): Expiry | null {
  const m = expires?.match(/^(\d{4})-(\d{2})-(\d{2})$/);
  if (!m) return null;
  const end = new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]));
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const days = Math.round((end.getTime() - today.getTime()) / 86_400_000);
  const date = end.toLocaleDateString("en-GB", { day: "numeric", month: "long", year: "numeric" });
  return { date, days, warn: days <= WARN_DAYS };
}

/** The warning on Home in the last days. */
export function expiryWarning(e: Expiry): string {
  if (e.days <= 0) return "This test version stops working after today.";
  if (e.days === 1) return "This test version stops working after tomorrow.";
  return `This test version works for ${e.days} more days, until ${e.date}.`;
}
