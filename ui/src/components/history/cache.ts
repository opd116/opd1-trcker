// The History list's pages, kept across its unmounts (opening a scoreboard re-keys the page): Back from a scoreboard
// draws the rows it had at once, so the router's scroll restore lands on them, and the scoreboard finds its own
// row's RR here. Refreshed in the background every time the list mounts.
import type { HistoryPage, HistoryQueue } from "../../lib/types";

export const historyCache: { queue: HistoryQueue; pages: Partial<Record<HistoryQueue, HistoryPage>> } = {
  queue: "competitive",
  pages: {},
};

/** The cached row of a match, from either queue's pages. */
export function cachedRow(matchId: string) {
  for (const p of Object.values(historyCache.pages)) {
    const r = p?.rows.find((x) => x.matchId === matchId);
    if (r) return r;
  }
  return undefined;
}
