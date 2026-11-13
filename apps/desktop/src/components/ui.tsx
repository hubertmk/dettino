import { useEffect, useRef, useState } from "react";
import { Info, AlertTriangle, CheckCircle2, XCircle } from "lucide-react";
import { ChordSpec, chordParts, vkFromEvent, MODS, api, keyName } from "../lib/api";

export function Toggle(p: { on: boolean; onChange: (v: boolean) => void; label: string; disabled?: boolean }) {
  return (
    <button
      role="switch"
      aria-checked={p.on}
      aria-label={p.label}
      disabled={p.disabled}
      className={`toggle ${p.on ? "on" : ""}`}
      onClick={() => p.onChange(!p.on)}
    />
  );
}

export function Seg<T extends string>(p: { value: T; options: { value: T; label: string }[]; onChange: (v: T) => void; label: string }) {
  return (
    <div className="seg" role="radiogroup" aria-label={p.label}>
      {p.options.map((o) => (
        <button key={o.value} role="radio" aria-checked={p.value === o.value} className={p.value === o.value ? "on" : ""} onClick={() => p.onChange(o.value)}>
          {o.label}
        </button>
      ))}
    </div>
  );
}

/** A row of colour dots that acts as a radio group. An option without colours is drawn as "automatic". */
export function Swatches<T extends string>(p: { value: T; options: { value: T; label: string; light?: string; dark?: string }[]; onChange: (v: T) => void; label: string }) {
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  const at = Math.max(0, p.options.findIndex((o) => o.value === p.value));
  const move = (d: number) => {
    const n = (at + d + p.options.length) % p.options.length;
    p.onChange(p.options[n].value);
    refs.current[n]?.focus();
  };
  return (
    <div
      className="swatches"
      role="radiogroup"
      aria-label={p.label}
      onKeyDown={(e) => {
        const d = e.key === "ArrowRight" || e.key === "ArrowDown" ? 1 : e.key === "ArrowLeft" || e.key === "ArrowUp" ? -1 : 0;
        if (d) {
          e.preventDefault();
          move(d);
        }
      }}
    >
      {p.options.map((o, i) => (
        <button
          key={o.value}
          ref={(el) => {
            refs.current[i] = el;
          }}
          role="radio"
          aria-checked={p.value === o.value}
          aria-label={o.label}
          title={o.label}
          tabIndex={i === at ? 0 : -1}
          className={`swatch${o.light ? "" : " auto"}${p.value === o.value ? " on" : ""}`}
          style={o.light ? ({ "--sw-light": o.light, "--sw-dark": o.dark ?? o.light } as React.CSSProperties) : undefined}
          onClick={() => p.onChange(o.value)}
        />
      ))}
    </div>
  );
}

export function Row(p: { title: React.ReactNode; desc?: React.ReactNode; children?: React.ReactNode }) {
  return (
    <div className="row">
      <div className="grow">
        <div className="title">{p.title}</div>
        {p.desc && <div className="desc">{p.desc}</div>}
      </div>
      {p.children}
    </div>
  );
}

export function Banner(p: { kind?: "info" | "warn" | "err" | "ok"; children: React.ReactNode }) {
  const Icon = p.kind === "warn" ? AlertTriangle : p.kind === "err" ? XCircle : p.kind === "ok" ? CheckCircle2 : Info;
  return (
    <div className={`banner ${p.kind ?? ""}`}>
      <Icon />
      <div className="grow">{p.children}</div>
    </div>
  );
}

export function Chord({ c }: { c: ChordSpec }) {
  const parts = chordParts(c);
  return (
    <span className="chord">
      {parts.map((k, i) => (
        <span key={i} className="chord">
          {i > 0 && <span className="plus">+</span>}
          <kbd className="kbd">{k}</kbd>
        </span>
      ))}
    </span>
  );
}

/** Records a chord. While open, the global hook is suspended so keys reach this window. */
export function KeyRecorder(p: { value: ChordSpec; onChange: (c: ChordSpec) => void; label: string }) {
  const [rec, setRec] = useState(false);
  const [live, setLive] = useState<string>("");
  const best = useRef<ChordSpec | null>(null);

  useEffect(() => {
    if (!rec) return;
    api.hotkeysSuspend(true);
    const held = new Set<number>();
    const down = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape" && !e.ctrlKey && !e.altKey && !e.shiftKey) {
        setRec(false);
        return;
      }
      const vk = vkFromEvent(e);
      if (vk === null) return;
      held.add(vk);
      const mods = (e.ctrlKey ? MODS.CTRL : 0) | (e.shiftKey ? MODS.SHIFT : 0) | (e.altKey ? MODS.ALT : 0) | (e.metaKey ? MODS.WIN : 0);
      const isMod = [0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0x5b, 0x5c, 0xff].includes(vk);
      if (isMod) {
        best.current = { mods: 0, key: vk };
        setLive(keyName(vk) + " …");
      } else {
        best.current = { mods, key: vk };
        setLive(chordParts(best.current).join(" + "));
      }
    };
    const up = (e: KeyboardEvent) => {
      e.preventDefault();
      const vk = vkFromEvent(e);
      if (vk !== null) held.delete(vk);
      if (held.size === 0 && best.current) {
        p.onChange(best.current);
        setRec(false);
      }
    };
    // Losing the window while recording would leave every shortcut suspended.
    const cancel = () => setRec(false);
    window.addEventListener("keydown", down, true);
    window.addEventListener("keyup", up, true);
    window.addEventListener("blur", cancel);
    return () => {
      window.removeEventListener("keydown", down, true);
      window.removeEventListener("keyup", up, true);
      window.removeEventListener("blur", cancel);
      api.hotkeysSuspend(false);
      best.current = null;
      setLive("");
    };
  }, [rec]);

  if (rec) {
    return (
      <span className="recorder" aria-live="polite">
        {live || "Press the shortcut…"}
        <span className="small muted">Esc to cancel</span>
      </span>
    );
  }
  return (
    <button className="btn ghost" aria-label={`Change shortcut for ${p.label}`} onClick={() => setRec(true)} title="Click to change">
      <Chord c={p.value} />
    </button>
  );
}

export function Sparkline({ values }: { values: number[] }) {
  if (values.length < 2) return <svg className="spark" />;
  const max = Math.max(...values, 1);
  const pts = values.map((v, i) => [(i / (values.length - 1)) * 72, 22 - (v / max) * 20]);
  const d = pts.map((p, i) => `${i ? "L" : "M"}${p[0].toFixed(1)},${p[1].toFixed(1)}`).join(" ");
  return (
    <svg className="spark" viewBox="0 0 72 24" aria-label="Recent latency">
      <path d={d} />
    </svg>
  );
}

export function Meter({ level }: { level: number }) {
  return (
    <div className="meter" role="meter" aria-valuenow={Math.round(level * 100)} aria-valuemin={0} aria-valuemax={100} aria-label="Microphone level">
      <span className="zone" />
      <i style={{ width: `${Math.round(Math.min(1, level) * 100)}%` }} />
    </div>
  );
}

export function Empty(p: { icon: React.ReactNode; title: string; children: React.ReactNode; action?: React.ReactNode }) {
  return (
    <div className="empty">
      <div className="art">{p.icon}</div>
      <h3>{p.title}</h3>
      <p>{p.children}</p>
      {p.action}
    </div>
  );
}

export function useToast() {
  const [msg, setMsg] = useState<string | null>(null);
  useEffect(() => {
    if (!msg) return;
    const t = setTimeout(() => setMsg(null), 2200);
    return () => clearTimeout(t);
  }, [msg]);
  const node = msg ? <div className="toast" role="status">{msg}</div> : null;
  return [node, setMsg] as const;
}

export function timeAgo(ms: number): string {
  const d = Date.now() - ms;
  if (d < 60_000) return "just now";
  if (d < 3_600_000) return `${Math.floor(d / 60_000)} min ago`;
  const date = new Date(ms);
  const today = new Date();
  if (date.toDateString() === today.toDateString()) return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  return date.toLocaleDateString([], { month: "short", day: "numeric" });
}

export function ms(n: number | null | undefined) {
  if (n == null) return "—";
  return n >= 1000 ? `${(n / 1000).toFixed(1)} s` : `${n} ms`;
}
