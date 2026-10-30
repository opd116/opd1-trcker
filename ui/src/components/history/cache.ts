// The History list's pages, kept across its unmounts (opening a scoreboard re-keys the page): Back from a scoreboard
// draws the rows it had at once, so the router's scroll restore lands on them, and the scoreboard finds its own
// row's RR here. Refreshed in the background every time the list mounts. The rows are the user's own matches, so the
// cache starts a new epoch (and empties itself) when the Riot account changes: a remount then draws no previous
// account's rows while its own first page travels, and a page asked for under the old epoch — by an instance the
// switch unmounted — can never write itself back (see `putHistoryPage`).
import { get } from "svelte/store";
import { onAccountChange, selfId } from "../../lib/stores";
import type { HistoryPage, HistoryQueue } from "../../lib/types";

export const historyCache: { queue: HistoryQueue; pages: Partial<Record<HistoryQueue, HistoryPage>>; epoch: number } = {
  queue: "competitive",
  pages: {},
  /** Bumped by every account change; a page asked for under an older epoch is dropped. */
  epoch: 0,
};

/** The account the current epoch belongs to (null before the first account is known). */
let epochAccount: string | null = null;

/**
 * Starts a new cache epoch when the account changed (`stores.selfId`) and returns the current one. Called by this
 * module's own account-change handler (the list may be unmounted) and by a mounted list's handler (it needs the new
 * epoch for its refresh); a second call for the same account is a no-op, so the two handlers' order does not matter.
 */
export function historyCacheEpoch(): number {
  const id = get(selfId);
  if (id !== epochAccount) {
    epochAccount = id;
    historyCache.pages = {};
    historyCache.epoch++;
  }
  return historyCache.epoch;
}

/** Writes a page asked for under `epoch`; a page of an older account's epoch is dropped (returns false). */
export function putHistoryPage(epoch: number, queue: HistoryQueue, page: HistoryPage): boolean {
  if (epoch !== historyCache.epoch) return false;
  historyCache.pages[queue] = page;
  return true;
}

onAccountChange(() => {
  historyCacheEpoch();
});

/** The cached row of a match, from either queue's pages. */
export function cachedRow(matchId: string) {
  for (const p of Object.values(historyCache.pages)) {
    const r = p?.rows.find((x) => x.matchId === matchId);
    if (r) return r;
  }
  return undefined;
}
