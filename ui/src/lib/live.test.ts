// The live card indicator rules (spec §3.5, §3.9).
import { describe, expect, it } from "vitest";
import { card as mk } from "./demo/fixtures";
import { showEncounters, showParty } from "./live";
import type { PlayerCard } from "./types";

const c = (puuid: string, extra: Partial<PlayerCard> = {}): PlayerCard => mk({ puuid, cardId: "card", ...extra });
const me = (extra: Partial<PlayerCard> = {}) => c("me", { isSelf: true, isSelfParty: true, ...extra });

describe("card indicators", () => {
  it("showEncounters / showParty suppress self and self-party", () => {
    const enc = { with: 1, against: 2 };
    const party = { group: 0, confirmed: true };
    expect(showEncounters(c("a", { encounters: enc }))).toBe(true);
    expect(showEncounters(c("a", { encounters: { with: 0, against: 0 } }))).toBe(false);
    expect(showEncounters(c("a", { encounters: null }))).toBe(false);
    expect(showEncounters(me({ encounters: enc }))).toBe(false);
    expect(showEncounters(c("a", { encounters: enc, isSelfParty: true }))).toBe(false);

    expect(showParty(c("a", { party }))).toBe(true);
    expect(showParty(c("a", { party: null }))).toBe(false);
    expect(showParty(me({ party }))).toBe(false);
    expect(showParty(c("a", { party, isSelfParty: true }))).toBe(false);
  });
});
