import type { Peak } from "./types";

/** Every missing value renders as this. */
export const DASH = "—";

/** U+2212, the typographic minus: same width as "+" in tabular figures. */
const MINUS = "−";

const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

type Num = number | null | undefined;

function ok(n: Num): n is number {
  return typeof n === "number" && Number.isFinite(n);
}

/** A duration as one short unit: 45m, 3h, 2d, 2w, 5mo, 1y. */
export function span(ms: Num): string {
  if (!ok(ms)) return DASH;
  const d = Math.max(0, ms);
  if (d < HOUR) return `${Math.max(1, Math.floor(d / MIN))}m`;
  if (d < DAY) return `${Math.floor(d / HOUR)}h`;
  const days = Math.floor(d / DAY);
  if (days < 7) return `${days}d`;
  if (days < 30) return `${Math.floor(days / 7)}w`;
  if (days < 365) return `${Math.floor(days / 30)}mo`;
  return `${Math.floor(days / 365)}y`;
}

/** Time since a timestamp (epoch ms), e.g. "3h". */
export function timeAgo(tsMs: Num, nowMs: number = Date.now()): string {
  if (!ok(tsMs)) return DASH;
  return span(nowMs - tsMs);
}

/** The backend builds act labels ("E8 A3"); the UI only passes them through. */
export function actLabel(label: string | null | undefined): string {
  return label ? label : DASH;
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/**
 * An act's date range: "19 Aug – 14 Oct 2026", or "23 Oct 2024 – 8 Jan 2025"
 * across new year. Act boundaries are UTC midnights, so read them in UTC.
 */
export function actDates(startMs: Num, endMs: Num): string {
  if (!ok(startMs) || !ok(endMs)) return DASH;
  const a = new Date(startMs);
  const b = new Date(endMs);
  const day = (d: Date) => `${d.getUTCDate()} ${MONTHS[d.getUTCMonth()]}`;
  const sameYear = a.getUTCFullYear() === b.getUTCFullYear();
  const from = sameYear ? day(a) : `${day(a)} ${a.getUTCFullYear()}`;
  return `${from} – ${day(b)} ${b.getUTCFullYear()}`;
}

/** "E8 A3 · 1y" (time since that act ended), or "This Act". */
export function peakLabel(peak: Peak | null | undefined): string {
  if (!peak) return DASH;
  if (peak.sinceActEndMs === null) return "This Act";
  return `${actLabel(peak.actLabel)} · ${span(peak.sinceActEndMs)}`;
}

/** "+16", "−19", "0". */
export function rrDelta(n: Num): string {
  if (!ok(n)) return DASH;
  const v = Math.round(n);
  if (v > 0) return `+${v}`;
  if (v < 0) return `${MINUS}${-v}`;
  return "0";
}

/** A 0..1 fraction as a percentage with one decimal: 0.545 → "54.5%". */
export function pct(fraction: Num): string {
  if (!ok(fraction)) return DASH;
  const tenths = Math.round(fraction * 1000);
  if (tenths >= 1000) return "100%";
  return `${(tenths / 10).toFixed(1)}%`;
}

/** K/D ratio with two decimals. */
export function kd(ratio: Num): string {
  if (!ok(ratio)) return DASH;
  return ratio.toFixed(2);
}

export function rr(n: Num): string {
  return ok(n) ? String(Math.round(n)) : DASH;
}

export function level(n: Num): string {
  return ok(n) ? String(Math.round(n)) : DASH;
}

/** valorant-api tier names are upper case ("DIAMOND 2") → "Diamond 2". */
export function tierName(raw: string | null | undefined): string {
  if (!raw) return DASH;
  return raw.toLowerCase().replace(/(^|\s)\p{L}/gu, (m) => m.toUpperCase());
}
