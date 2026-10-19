/** Stable per-user hue so a name keeps its color across sessions. */
export function userColor(name: string): string {
  let h = 2166136261;
  for (let i = 0; i < name.length; i++) {
    h ^= name.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return `hsl(${(h >>> 0) % 360} 70% var(--user-l))`;
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB"];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v < 10 ? 1 : 0)} ${units[i]}`;
}

export function formatEta(remainingBytes: number, bps: number): string {
  if (bps <= 0) return "—";
  const s = Math.round(remainingBytes / bps);
  return s >= 60 ? `${Math.floor(s / 60)}m ${s % 60}s` : `${s}s`;
}

const CHANNEL_RE = /^[a-z0-9_]{1,25}$/;
/** Mirrors the backend rule; returns an error message or null. */
export function channelError(raw: string): string | null {
  const v = raw.trim().replace(/^#/, "").toLowerCase();
  if (!v) return "Enter a channel name.";
  return CHANNEL_RE.test(v) ? null : "Use 1–25 letters, digits or underscores.";
}
