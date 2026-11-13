// The speech-bubble d from site/icon.svg.
import { C } from "../theme";

export function Mark({ size }: { size: number }) {
  return (
    <svg viewBox="0 0 1024 1024" width={size} height={size}>
      <rect x="64" y="64" width="896" height="896" rx="208" fill={C.accent} />
      <path d="M319 716L250 846L407 777Z" fill="#fff" stroke="#fff" strokeWidth="22" strokeLinejoin="round" />
      <circle cx="478" cy="582" r="208" fill="#fff" />
      <rect x="606" y="182" width="160" height="612" rx="80" fill="#fff" />
      <circle cx="478" cy="582" r="88" fill={C.accent} />
    </svg>
  );
}
