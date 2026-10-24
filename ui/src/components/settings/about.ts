// The About page's rows (design v2 `aboutInfo`, TrackerApp.dc.html 2040: the labels and their order; the design has
// no markup for it). Pure: About.svelte draws them. Missing values read "—".
import { dateLong } from "../../lib/format";
import type { AppInfo, AppStatus, ConnState } from "../../lib/types";

const DASH = "—";

export type AboutRow = { label: string; value: string; conn?: ConnState };

/** "48 MB", "3.4 MB", "820 KB"; "—" for a missing or negative size. */
export function dbSize(bytes: number | null | undefined): string {
  if (typeof bytes !== "number" || !Number.isFinite(bytes) || bytes < 0) return DASH;
  const mb = bytes / (1024 * 1024);
  if (mb >= 10) return `${Math.round(mb)} MB`;
  if (mb >= 1) return `${(Math.round(mb * 10) / 10).toFixed(1)} MB`;
  return `${Math.max(bytes > 0 ? 1 : 0, Math.round(bytes / 1024))} KB`;
}

/** The connection, as the About row says it after the Riot Client version. */
export const CONN_WORD: Record<ConnState, string> = {
  ok: "connected",
  waiting: "connected",
  lost: "reconnecting",
  error: "unreachable",
};

/** `release-11.08` → `11.08` (the design's VALORANT row). */
export const gameVersion = (v: string | null | undefined): string | null => (v ? v.replace(/^release-/i, "") : null);

const join = (...parts: (string | null | undefined)[]) => parts.filter((p): p is string => !!p).join(" · ") || DASH;

export function aboutRows(info: AppInfo | null, status: AppStatus | null): AboutRow[] {
  const riot = info?.riotClientVersion ?? status?.riotClientVersion ?? null;
  const conn = status?.conn;
  const game = gameVersion(info?.valorantVersion ?? status?.gameVersion ?? null);
  const db =
    info && Number.isFinite(info.dbMatches)
      ? `${info.dbMatches.toLocaleString("en-US")} ${info.dbMatches === 1 ? "match" : "matches"} · ${dbSize(info.dbBytes)}`
      : DASH;
  return [
    { label: "VERSION", value: join(info?.version, info?.commit) },
    { label: "BUILD", value: info?.buildMs ? dateLong(info.buildMs) : DASH },
    { label: "RIOT CLIENT", value: join(riot, conn ? CONN_WORD[conn] : null), conn },
    { label: "VALORANT", value: game ?? DASH },
    { label: "GAME DATA", value: "valorant-api.com" },
    { label: "LOCAL DATABASE", value: db },
  ];
}
