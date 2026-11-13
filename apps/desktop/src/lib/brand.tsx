/** The product name shown to people. Identifiers (bundle id, data folder, registry value) stay unchanged. */
export const APP_NAME = "Dettino";

/** Opened in the system browser from About only when the user clicks; the app never contacts it on its own. */
export const BUY_ME_A_COFFEE_URL = "https://buymeacoffee.com/hubertmk";

/** The speech-bubble d: a lowercase d whose bowl is a speech bubble. Drawn in the accent the user picked, so Cobalt by default (as the app icon). */
export function BrandMark({ size = 26, className }: { size?: number; className?: string }) {
  return (
    <svg className={className} width={size} height={size} viewBox="0 0 1024 1024" aria-hidden>
      <rect x="64" y="64" width="896" height="896" rx="208" style={{ fill: "var(--accent)" }} />
      <path d="M319 716L250 846L407 777Z" fill="#ffffff" stroke="#ffffff" strokeWidth="22" strokeLinejoin="round" />
      <circle cx="478" cy="582" r="208" fill="#ffffff" />
      <rect x="606" y="182" width="160" height="612" rx="80" fill="#ffffff" />
      <circle cx="478" cy="582" r="88" style={{ fill: "var(--accent)" }} />
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
