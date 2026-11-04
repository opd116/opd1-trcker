// Central metric-colour bands (R207, broadened white range approved in R209): the Live row's 5-match K/D and headshot %, the profile
// drawer's hero (current-act, up to 20 matches) K/D and headshot %, and the full current act's win rate on both pages
// each map a raw metric value onto one of six literal colours. Pure and typed: bands are chosen from the raw value,
// never from formatted or rounded text, and every band is [lower, next) — lower bound inclusive, upper exclusive.
//
// An unknown value never colours: pending (undefined), unknown (null), NaN, ±Infinity, a negative, or a fraction above
// 1 all stay the context's neutral (never the bad red). With the "Highlight standout stats" setting off every value is
// neutral again, exactly as before this mapping. K/D is a ratio (finite, ≥ 0, unbounded above); headshot % and win
// rate are fractions (0..1). Imports only the palette — the theme's own `hl` helpers stay untouched for other pages.
import { PALETTE } from "./theme";

/** The six band colours, worst → best (0: dark red #D32F2F, 1: red #FF5252, 2: orange #FF9100, 3: white #FFFFFF, 4: light green #69E08A, 5: strong green #00C853). */
export const METRIC_COLORS = ["#D32F2F", "#FF5252", "#FF9100", "#FFFFFF", "#69E08A", "#00C853"] as const;

/** The five inclusive lower bounds of a scope's six bands: value ≥ bands[i] is at least band i+1. */
export type MetricBands = readonly [number, number, number, number, number];

// ── the scopes (Requirement R2 / Decision R210) ──────────────────────────────────────────────────────────────────
/** LIVE row (last 5 matches): K/D. */
export const LIVE_KD_BANDS: MetricBands = [0.72, 0.79, 0.87, 1.15, 1.39];
/** LIVE row (last 5 matches): headshot fraction. */
export const LIVE_HS_BANDS: MetricBands = [0.15, 0.175, 0.205, 0.275, 0.33];
/** PROFILE hero stats (current act, up to 20 matches): K/D. */
export const PROFILE_KD_BANDS: MetricBands = [0.81, 0.86, 0.92, 1.10, 1.24];
/** PROFILE hero stats (current act, up to 20 matches): headshot fraction. */
export const PROFILE_HS_BANDS: MetricBands = [0.16, 0.185, 0.21, 0.27, 0.32];
/** Win rate over the full current act — the same bands on the Live row and in the profile. */
export const WR_BANDS: MetricBands = [0.36, 0.40, 0.44, 0.56, 0.64];

/** A metric as the wire carries it: absent while loading (undefined), unknown (null), else a number. */
export type MetricValue = number | null | undefined;

/** The Live row's neutral: its plain stat colour today, both while pending and with the highlight off. */
export const LIVE_NEUTRAL = PALETTE.textSoft;
/** The profile's neutral while the highlight is on (pending or unknown); off keeps its own softer fallback. */
export const PROFILE_NEUTRAL_ON = "var(--text-mid)";
/** The profile's neutral with the highlight setting off (unchanged by this mapping). */
export const PROFILE_NEUTRAL_OFF = "var(--text-soft)";

const profileNeutral = (on: boolean): string => (on ? PROFILE_NEUTRAL_ON : PROFILE_NEUTRAL_OFF);

function bandOf(v: MetricValue, bands: MetricBands, unbounded: boolean): number | null {
  if (typeof v !== "number" || !Number.isFinite(v) || v < 0) return null;
  if (!unbounded && v > 1) return null; // a fraction above 100% cannot be true; a K/D may be
  let band = 0;
  for (const t of bands) if (v >= t) band++;
  return band;
}

/** Band index 0–5 for a valid fraction (0..1), or null when unknown/invalid (never colours). */
export const fractionBand = (v: MetricValue, bands: MetricBands): number | null => bandOf(v, bands, false);

/** Band index 0–5 for a valid non-negative ratio (unbounded above), or null when unknown/invalid (never colours). */
export const ratioBand = (v: MetricValue, bands: MetricBands): number | null => bandOf(v, bands, true);

function color(band: number | null, on: boolean, neutral: string): string {
  if (!on || band === null) return neutral;
  return METRIC_COLORS[band]!;
}

// ── the Live row's colours (5-match scope; neutral = the row's plain stat colour) ─────────────────────────────────
/** LIVE K/D: the exact 5-match bands with the highlight on, the row's neutral otherwise. */
export const liveKdColor = (kd: MetricValue, on: boolean): string => color(ratioBand(kd, LIVE_KD_BANDS), on, LIVE_NEUTRAL);
/** LIVE headshot fraction: the exact 5-match bands with the highlight on, the row's neutral otherwise. */
export const liveHsColor = (hs: MetricValue, on: boolean): string => color(fractionBand(hs, LIVE_HS_BANDS), on, LIVE_NEUTRAL);
/** LIVE full-current-act win rate: the shared WR bands with the highlight on, the row's neutral otherwise. */
export const liveWrColor = (wr: MetricValue, on: boolean): string => color(fractionBand(wr, WR_BANDS), on, LIVE_NEUTRAL);

// ── the profile's colours (current-act hero scope; pending stays the profile's own neutral) ───────────────────────
/** PROFILE K/D: the exact hero bands with the highlight on, the profile's neutral otherwise. */
export const profileKdColor = (kd: MetricValue, on: boolean): string =>
  color(ratioBand(kd, PROFILE_KD_BANDS), on, profileNeutral(on));
/** PROFILE headshot fraction: the exact hero bands with the highlight on, the profile's neutral otherwise. */
export const profileHsColor = (hs: MetricValue, on: boolean): string =>
  color(fractionBand(hs, PROFILE_HS_BANDS), on, profileNeutral(on));
/** PROFILE full-current-act win rate: the shared WR bands with the highlight on, the profile's neutral otherwise. */
export const profileWrColor = (wr: MetricValue, on: boolean): string => color(fractionBand(wr, WR_BANDS), on, profileNeutral(on));
