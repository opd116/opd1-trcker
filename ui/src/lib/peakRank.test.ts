import { describe, expect, it } from 'vitest';
import { peakRank } from './peakRank';
import type { Peak } from './types';
const at = (tier:number): Peak => ({tier,actLabel:'V26 A5',actName:'V26 ACT V',sinceActEndMs:null,actStartMs:1,actEndMs:2});
describe('compact peak metadata', () => {
  it.each([[3,1],[5,3],[18,1],[20,3],[21,1],[22,2],[23,3],[24,1],[25,2],[26,3],[27,3]])('tier %i has normalized division %i', (tier,division) => {
    expect(peakRank(at(tier),null)?.division).toBe(division);
  });
  it('does not fabricate missing/unranked peaks', () => {
    for (const peak of [undefined,null,at(0),at(1),at(2)]) expect(peakRank(peak,null)).toBeNull();
  });
  it('keeps the full act name in the accessible tooltip', () => {
    expect(peakRank(at(24),null)?.tip).toContain('V26 ACT V');
    expect(peakRank(at(24),null)?.tip).toContain('This act');
  });
  it('shows the actual act label even while that act is current', () => {
    expect(peakRank(at(24),null)?.actAge).toBe('V26 A5 · This act');
  });
  it.each([[1,'1d'],[30,'1mo'],[365,'1y']])('formats %i elapsed days without fabricating an episode', (days,label) => {
    const peak = { ...at(24), actLabel:'E9 A3', actName:'EPISODE 9 ACT III', sinceActEndMs: Number(days)*86_400_000 };
    expect(peakRank(peak,null)?.actAge).toBe(`E9 A3 · ${label} ago`);
    expect(peakRank(peak,null)?.tip).toContain('EPISODE 9 ACT III');
  });
  it.each([NaN,Infinity,-1])('does not invent a relative age for %s', (elapsed) => {
    expect(peakRank({ ...at(24), sinceActEndMs:elapsed },null)?.actAge).toBe('V26 A5 · —');
  });
});
