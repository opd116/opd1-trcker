// String formatters (spec §4.3). Every missing value renders as DASH.
/** Every missing value renders as this. */
export const DASH = "—";

/** U+2212, the typographic minus: same width as "+" in tabular figures. */
const MINUS = "−";
/** U+2013, between the two numbers of a score and the two ends of a date range. */
const EN = "–";

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

/** Time since a timestamp (epoch ms), e.g. "3h ago". */
export function ago(tsMs: Num, nowMs: number = Date.now()): string {
  if (!ok(tsMs)) return DASH;
  return `${span(nowMs - tsMs)} ago`;
}

/** "+18", "−15", "0". */
export function rrDelta(n: Num): string {
  if (!ok(n)) return DASH;
  const v = Math.round(n);
  if (v > 0) return `+${v}`;
  if (v < 0) return `${MINUS}${-v}`;
  return "0";
}

/** A 0..1 fraction as a whole percentage: 0.639 → "64%". */
export function pct0(fraction: Num): string {
  if (!ok(fraction)) return DASH;
  return `${Math.round(fraction * 100)}%`;
}

/** K/D ratio with two decimals. */
export function kd2(ratio: Num): string {
  if (!ok(ratio)) return DASH;
  return ratio.toFixed(2);
}

/** The word for `n` of it: "game" for 1, "games" otherwise. */
export function plural(n: number, word: string): string {
  return n === 1 ? word : `${word}s`;
}

/** "1 game", "0 games", "3 ranked players". */
export function count(n: Num, word: string): string {
  if (!ok(n)) return DASH;
  return `${n} ${plural(n, word)}`;
}

/** "13–9". */
export function score(a: Num, b: Num): string {
  if (!ok(a) || !ok(b)) return DASH;
  return `${a}${EN}${b}`;
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

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/**
 * An act's date range in UTC (act boundaries are UTC midnights): "Aug 26 – Oct 20, 2026", or
 * "Nov 12, 2025 – Jan 6, 2026" across a new year. The end is the day of `actEndMs − 1`.
 */
export function actDates(startMs: Num, endMs: Num): string {
  if (!ok(startMs) || !ok(endMs)) return DASH;
  const a = new Date(startMs);
  const b = new Date(endMs - 1);
  const day = (d: Date) => `${MONTHS[d.getUTCMonth()]} ${d.getUTCDate()}`;
  const from = a.getUTCFullYear() === b.getUTCFullYear() ? day(a) : `${day(a)}, ${a.getUTCFullYear()}`;
  return `${from} ${EN} ${day(b)}, ${b.getUTCFullYear()}`;
}

const ROMAN: Record<string, number> = { I: 1, II: 2, III: 3, IV: 4, V: 5, VI: 6 };

/** "V26 ACT V" → "VALORANT 2026 · Act 5"; "EPISODE 9 ACT III" → "Episode 9 · Act 3"; anything else as given. */
export function actTitle(actName: string): string {
  const v = /^V(\d{2}) ACT ([IVX]+)$/i.exec(actName);
  if (v && ROMAN[v[2]!.toUpperCase()]) return `VALORANT 20${v[1]} · Act ${ROMAN[v[2]!.toUpperCase()]}`;
  const e = /^EPISODE (\d+) ACT ([IVX]+)$/i.exec(actName);
  if (e && ROMAN[e[2]!.toUpperCase()]) return `Episode ${e[1]} · Act ${ROMAN[e[2]!.toUpperCase()]}`;
  return actName;
}

// Local time, en-US, written out (no Intl: it puts a narrow no-break space before AM/PM).

/** "Sep 29, 2026". */
export function dateLong(tsMs: Num): string {
  if (!ok(tsMs)) return DASH;
  const d = new Date(tsMs);
  return `${MONTHS[d.getMonth()]} ${d.getDate()}, ${d.getFullYear()}`;
}

/** "Sep 12", plus ", YYYY" when it is not the current year. */
export function dateShort(tsMs: Num, nowMs: number = Date.now()): string {
  if (!ok(tsMs)) return DASH;
  const d = new Date(tsMs);
  const day = `${MONTHS[d.getMonth()]} ${d.getDate()}`;
  return d.getFullYear() === new Date(nowMs).getFullYear() ? day : `${day}, ${d.getFullYear()}`;
}

/** "9:14 PM". */
export function time12(tsMs: Num): string {
  if (!ok(tsMs)) return DASH;
  const d = new Date(tsMs);
  const h = d.getHours();
  return `${h % 12 === 0 ? 12 : h % 12}:${String(d.getMinutes()).padStart(2, "0")} ${h < 12 ? "AM" : "PM"}`;
}

/** "38 min". */
export function durationMin(ms: Num): string {
  if (!ok(ms)) return DASH;
  return `${Math.round(ms / MIN)} min`;
}

/**
 * How long ago, stepping up through the units: "42 s", "3 min", "2 h", "4 d" (whole units, rounded down past a
 * minute). Shared by the lost banner ("Showing data from 3 min ago") and the status pill's tooltip ("Last update 2 h
 * ago"), so a long-idle app never reads "1440 min".
 */
export function ageShort(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000));
  if (s < 60) return `${s} s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} min`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h} h`;
  return `${Math.floor(h / 24)} d`;
}

/** The tail every lockfile path ends with: the backend builds it as `%LOCALAPPDATA%` + this (poll.rs, state.rs). */
const LOCKFILE_TAIL = String.raw`\Riot Games\Riot Client\Config\lockfile`;

/**
 * The lockfile path in the design's short form, `%LOCALAPPDATA%\Riot Games\Riot Client\Config\lockfile`, when it is
 * the standard one under the user's LocalAppData (the backend always builds it from the `LOCALAPPDATA` variable, so a
 * path ending in the standard tail starts with that folder); any other path as given. The full path stays in a tooltip.
 */
export function shortLockfilePath(path: string): string {
  if (path.startsWith("%LOCALAPPDATA%")) return path;
  const i = path.length - LOCKFILE_TAIL.length;
  if (i <= 0 || path.slice(i).toLowerCase() !== LOCKFILE_TAIL.toLowerCase()) return path;
  return `%LOCALAPPDATA%${path.slice(i)}`;
}
