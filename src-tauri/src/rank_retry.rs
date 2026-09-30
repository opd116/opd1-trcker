//! R155: bounded retries of a failed rank sync.
//!
//! A per-player sync whose `comp_updates` or `mmr` call failed (any error but
//! `ClientGone`) leaves the card without its MMR fields. Instead of waiting for
//! the next roster or match, the player is synced again later: at most
//! [`RANK_RETRY_DELAYS`]`.len()` more times, the first no sooner than 20 s
//! after the failure, the second no sooner than 60 s after the first retry
//! failed. The three syncs are the most a player ever gets from one first
//! pass, whatever happens afterwards: a retry is counted when it *starts*
//! ([`RankRetries::started`]), so one that is cut short (the worker is aborted
//! by a stage change) still uses up its turn, and a retry that fails again only
//! moves the schedule on — it never starts a new round.
//!
//! This module is only the schedule: which players wait, and until when. It
//! takes the current time as an argument and never reads a clock or a lock, so
//! its tests move time by passing later instants. Whether a player is still on
//! the live roster, and whether VALORANT is closed, is `LiveModel`'s call
//! (`LiveModel::note_rank_sync`, `LiveModel::due_rank_retry`).

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// The wait before each retry, counted from the failure it follows: the first
/// retry 20 s after the failed first sync, the second 60 s after the first
/// retry failed. Its length is the number of retries a player gets.
pub const RANK_RETRY_DELAYS: [Duration; 2] = [Duration::from_secs(20), Duration::from_secs(60)];

/// One player waiting for a retry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Waiting {
    /// Retries already run for this first pass (0 before the first retry).
    done: usize,
    /// The retry may run from this instant on.
    due: Instant,
}

/// The players waiting for a rank retry, and when each is due.
#[derive(Debug, Default)]
pub struct RankRetries {
    waiting: HashMap<String, Waiting>,
}

impl RankRetries {
    /// A first-pass sync of `puuid` ended at `now` (`failed`: its rank part
    /// did). A failure starts a fresh round — the first retry is due
    /// [`RANK_RETRY_DELAYS`]`[0]` from now, whatever was pending for the
    /// player before; a success ends it.
    pub fn first_pass(&mut self, puuid: &str, failed: bool, now: Instant) {
        if failed {
            self.waiting.insert(
                puuid.to_string(),
                Waiting {
                    done: 0,
                    due: now + RANK_RETRY_DELAYS[0],
                },
            );
        } else {
            self.waiting.remove(puuid);
        }
    }

    /// A retry of `puuid` starts at `now` (its worker was handed the job).
    /// The retry is used up from here on, however it ends. The next one, if
    /// the round has another, is due [`RANK_RETRY_DELAYS`] after this start
    /// — at the earliest; [`Self::finished`] moves it to after the failure.
    /// After the last retry nothing is left pending. Nothing happens for a
    /// player with no round pending.
    pub fn started(&mut self, puuid: &str, now: Instant) {
        let Some(w) = self.waiting.get_mut(puuid) else {
            return;
        };
        w.done += 1;
        match RANK_RETRY_DELAYS.get(w.done) {
            Some(delay) => w.due = now + *delay,
            None => {
                self.waiting.remove(puuid);
            }
        }
    }

    /// A retry of `puuid` ended at `now`. A success ends the round; a failure
    /// makes the next retry (if the round has another) due
    /// [`RANK_RETRY_DELAYS`] after *this* failure. It never starts a round: a
    /// player with none pending (the last retry, or a round dropped
    /// meanwhile) stays without.
    pub fn finished(&mut self, puuid: &str, failed: bool, now: Instant) {
        let Some(w) = self.waiting.get_mut(puuid) else {
            return;
        };
        match RANK_RETRY_DELAYS.get(w.done) {
            Some(delay) if failed => w.due = now + *delay,
            _ => {
                self.waiting.remove(puuid);
            }
        }
    }

    /// How many retries `puuid` has run in its pending round (`None`: no round
    /// is pending).
    pub fn retries_done(&self, puuid: &str) -> Option<usize> {
        self.waiting.get(puuid).map(|w| w.done)
    }

    /// Whether `puuid`'s retry may run at `now`.
    pub fn is_due(&self, puuid: &str, now: Instant) -> bool {
        self.waiting.get(puuid).is_some_and(|w| w.due <= now)
    }

    /// Ends `puuid`'s pending round, if any.
    pub fn forget(&mut self, puuid: &str) {
        self.waiting.remove(puuid);
    }

    /// Keeps only the players `keep` says yes to.
    pub fn retain(&mut self, mut keep: impl FnMut(&str) -> bool) {
        self.waiting.retain(|puuid, _| keep(puuid));
    }

    /// Ends every pending round.
    pub fn clear(&mut self) {
        self.waiting.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.waiting.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: Duration = Duration::from_secs(1);

    /// One retry as the worker runs it: handed out at `at`, over `took` later.
    fn retry(r: &mut RankRetries, p: &str, failed: bool, at: Instant, took: Duration) {
        r.started(p, at);
        r.finished(p, failed, at + took);
    }

    #[test]
    fn a_failed_first_pass_is_retried_20s_later_then_60s_after_that_and_never_again() {
        let t0 = Instant::now();
        let mut r = RankRetries::default();
        r.first_pass("p", true, t0);
        assert_eq!(r.retries_done("p"), Some(0));
        assert!(!r.is_due("p", t0 + 19 * S), "not before 20 s");
        assert!(r.is_due("p", t0 + 20 * S), "from 20 s on");

        // The first retry runs late (a busy worker), takes 3 s and fails.
        let t1 = t0 + 25 * S;
        r.started("p", t1);
        assert_eq!(r.retries_done("p"), Some(1));
        assert!(!r.is_due("p", t1 + 59 * S), "not while it is running");
        r.finished("p", true, t1 + 3 * S);
        assert!(!r.is_due("p", t1 + 62 * S), "60 s after the retry failed");
        assert!(r.is_due("p", t1 + 63 * S));

        // The second (last) retry: nothing is pending once it starts, so
        // nothing more is ever scheduled, whatever becomes of it.
        let t2 = t1 + 70 * S;
        r.started("p", t2);
        assert_eq!(r.retries_done("p"), None);
        r.finished("p", true, t2 + S);
        assert!(r.is_empty());
        assert!(!r.is_due("p", t2 + 10_000 * S));
    }

    #[test]
    fn a_retry_that_never_finishes_still_uses_up_its_turn() {
        // A worker aborted by a stage change mid-retry: the turn is spent
        // and the next one is 60 s after this one started, not at once.
        let t0 = Instant::now();
        let mut r = RankRetries::default();
        r.first_pass("p", true, t0);
        r.started("p", t0 + 20 * S);
        assert!(!r.is_due("p", t0 + 20 * S), "not straight away");
        assert!(!r.is_due("p", t0 + 79 * S));
        assert!(r.is_due("p", t0 + 80 * S));
        r.started("p", t0 + 80 * S);
        assert!(r.is_empty(), "the second and last turn is spent too");
    }

    #[test]
    fn a_success_ends_the_round_at_any_point() {
        let t0 = Instant::now();
        let mut r = RankRetries::default();
        // A first pass that worked schedules nothing (and ends an old round).
        r.first_pass("p", false, t0);
        assert!(r.is_empty());
        r.first_pass("p", true, t0);
        r.first_pass("p", false, t0 + S);
        assert!(r.is_empty());
        // A retry that worked ends it.
        r.first_pass("p", true, t0);
        retry(&mut r, "p", false, t0 + 20 * S, S);
        assert!(r.is_empty());
        // So does one of the second.
        r.first_pass("p", true, t0);
        retry(&mut r, "p", true, t0 + 20 * S, S);
        assert_eq!(r.retries_done("p"), Some(1));
        r.started("p", t0 + 81 * S);
        r.finished("p", false, t0 + 82 * S);
        assert!(r.is_empty());
    }

    #[test]
    fn a_retry_never_starts_a_round_of_its_own() {
        // A retry reported for a player with nothing pending (a round dropped
        // meanwhile) schedules nothing: only a first pass starts a round.
        let mut r = RankRetries::default();
        retry(&mut r, "p", true, Instant::now(), S);
        assert!(r.is_empty());
    }

    #[test]
    fn a_new_first_pass_failure_restarts_the_round() {
        let t0 = Instant::now();
        let mut r = RankRetries::default();
        r.first_pass("p", true, t0);
        retry(&mut r, "p", true, t0 + 20 * S, S);
        assert_eq!(r.retries_done("p"), Some(1));
        // The roster was re-synced from scratch (a new match): a fresh round.
        let t1 = t0 + 500 * S;
        r.first_pass("p", true, t1);
        assert_eq!(r.retries_done("p"), Some(0));
        assert!(!r.is_due("p", t1 + 19 * S));
        assert!(r.is_due("p", t1 + 20 * S));
    }

    #[test]
    fn players_are_independent_and_can_be_dropped() {
        let t0 = Instant::now();
        let mut r = RankRetries::default();
        r.first_pass("a", true, t0);
        r.first_pass("b", true, t0 + 10 * S);
        assert!(r.is_due("a", t0 + 20 * S) && !r.is_due("b", t0 + 20 * S));
        r.retain(|p| p == "b");
        assert!(!r.is_due("a", t0 + 100 * S), "a left the roster");
        assert!(r.is_due("b", t0 + 100 * S));
        r.forget("b");
        assert!(r.is_empty());
        r.first_pass("c", true, t0);
        r.clear();
        assert!(r.is_empty());
    }
}
