// The live card indicator rules (spec §3.5, §3.9), shared by the v2 rows (livev2.ts).
import type { PlayerCard } from "./types";

/** ⟲ badge: never on the user or the user's own party, and only with at least one shared match (spec §3.5). */
export const showEncounters = (c: PlayerCard): boolean =>
  !c.isSelf && !c.isSelfParty && !!c.encounters && c.encounters.with + c.encounters.against > 0;

/** Party stripe and tab: never on the user or the user's own party (spec §3.9). */
export const showParty = (c: PlayerCard): boolean => !!c.party && !c.isSelf && !c.isSelfParty;
