/** The product name shown to people. Identifiers (bundle id, data folder, registry value) stay unchanged. */
export const APP_NAME = "Dettino";

/** Opened in the system browser from About only when the user clicks; the app never contacts it on its own. */
export const BUY_ME_A_COFFEE_URL = "https://buymeacoffee.com/hubertmk";

const BRAND_TILE = "#2EC4A0";
const BRAND_INK = "#0B0D0C";

/** The speech-bubble d: a lowercase d whose bowl is a speech bubble. Always the brand colours
 *  (Seafoam tile, near-black d), exactly like the app icon (src-tauri/icons/icon.svg), whatever
 *  accent the user picked: it is the logo, not a themed control. */
export function BrandMark({ size = 26, className }: { size?: number; className?: string }) {
  return (
    <svg className={className} width={size} height={size} viewBox="0 0 1024 1024" aria-hidden>
      <rect x="64" y="64" width="896" height="896" rx="208" fill={BRAND_TILE} />
      <path d="M319 716L250 846L407 777Z" fill={BRAND_INK} stroke={BRAND_INK} strokeWidth="22" strokeLinejoin="round" />
      <circle cx="478" cy="582" r="208" fill={BRAND_INK} />
      <rect x="606" y="182" width="160" height="612" rx="80" fill={BRAND_INK} />
      <circle cx="478" cy="582" r="88" fill={BRAND_TILE} />
    </svg>
  );
}

export type Os = "windows" | "macos" | "linux";

export function osName(platform: string) {
  return platform === "macos" ? "macOS" : platform === "linux" ? "Linux" : "Windows";
}

/** Where API keys live on this OS, as people know it. */
export function keychainName(platform: string) {
  return platform === "macos" ? "macOS Keychain" : platform === "linux" ? "system keyring" : "Windows Credential Manager";
}
