import { actDates, span } from './format';
import { isRankedTier } from './meta';
import { tierColor, tierName } from './theme';
import type { Meta, Peak } from './types';

/** Peak rank metadata; the visible subtitle is the actual act label and age since that act ended (R207). */
export function peakRank(peak: Peak | null | undefined, meta: Meta | null) {
  if (!peak || !isRankedTier(peak.tier) || !Number.isInteger(peak.tier) || peak.tier > 27) return null;
  const division = peak.tier >= 27 ? 3 : ((peak.tier - 3) % 3) + 1;
  const elapsed = peak.sinceActEndMs;
  const age = elapsed === null ? 'This act' : Number.isFinite(elapsed) && elapsed >= 0 ? `${span(elapsed)} ago` : '—';
  const actAge = `${peak.actLabel} · ${age}`;
  return { tier: peak.tier, division, color: tierColor(peak.tier), name: tierName(peak.tier, meta),
    actAge, tip: `${tierName(peak.tier, meta)} · ${peak.actName}\n${actAge}\n${actDates(peak.actStartMs, peak.actEndMs)}` };
}
