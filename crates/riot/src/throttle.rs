//! A token-bucket rate limiter with a `hold_for` escape hatch for `429
//! Retry-After` responses.
//!
//! Mirrors Valorant-Scout's `backend/riot_client.py` throttle:
//! `_take_token` (burst + linear refill at `rps` tokens/sec, blocking the
//! caller until a token is free) and `_set_hold`/`held_secs` (a hard floor
//! on the next `take()`, extended — never shrunk — by repeated holds).
//!
//! R160: the hold is its own type, [`Hold`], so the session can keep one per
//! request *family* (`mmr` and `other`, like Scout's `_HOLD_UNTIL[fam]`) while
//! the token bucket stays shared. [`Throttle`] embeds one for its own users.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use tokio::time::sleep;

use crate::locks::locked;

/// The smallest `rps` a [`Throttle`] will ever actually use. Guards
/// against a nonsensical `rps <= 0` (or NaN) causing a divide-by-zero (or
/// by-a-negative-number) when computing how long to wait for the next
/// token — which would hand a non-finite or negative value to
/// `Duration::from_secs_f64`, and panic. Release builds unwind on panic
/// (R138: no `panic = "abort"`), so a panic here would end the calling task
/// instead of the app, but it would still cut that task short mid-match.
/// Real call sites always pass a real positive `rps` (10.0, 0.4); this only
/// protects against misuse.
const MIN_RPS: f64 = 0.01;

/// The longest a single `hold_for` call will ever actually hold,
/// regardless of what it's asked for (R160: Riot's `/mmr/` block is about
/// 60 s, so the old 30 s cap just earned another 429). A sanity ceiling, not a
/// policy: it guards against a malformed or absurd caller value
/// (`f64::INFINITY`, `f64::NAN`, a huge number — e.g. from a bogus
/// `Retry-After` header) turning into a `Duration`/`Instant` that panics, or
/// an effectively-infinite hold.
pub const MAX_HOLD_SECS: f64 = 120.0;

/// Clamps a `hold_for` duration to `[0, MAX_HOLD_SECS]`. `NaN` (which
/// `f64::clamp` passes through unchanged rather than clamping) maps to
/// `0.0` — we can't infer intent from a `NaN`, so we treat it as "no
/// hold" rather than "the maximum hold".
fn clamp_hold_secs(secs: f64) -> f64 {
    if secs.is_nan() {
        0.0
    } else {
        secs.clamp(0.0, MAX_HOLD_SECS)
    }
}

#[derive(Debug)]
struct Inner {
    /// Tokens currently available, in `[0, burst]`.
    tokens: f64,
    /// When `tokens` was last recomputed. `None` until the first `take()`
    /// (mirrors Scout's `bucket["at"] == 0` sentinel), so an idle throttle
    /// doesn't "earn" refill credit for the time before it was ever used.
    last_refill: Option<Instant>,
}

/// A hard floor on "when may the next call go out", set by a `429
/// Retry-After` and only ever extended, never shrunk. Independent of any token
/// bucket: the session keeps one per request family (R160).
#[derive(Debug, Default)]
pub struct Hold {
    until: Mutex<Option<Instant>>,
}

impl Hold {
    pub fn new() -> Self {
        Self::default()
    }

    /// Holds for at least `secs` from now (clamped to `[0, MAX_HOLD_SECS]`).
    /// A shorter hold never shrinks an already-active longer one — Scout's
    /// `_HOLD_UNTIL[fam] = max(_HOLD_UNTIL[fam], now + seconds)`. Returns what
    /// is left of the hold afterwards (zero when `secs` came to nothing and
    /// no hold was active).
    pub fn hold_for(&self, secs: f64) -> Duration {
        self.hold_until(Instant::now(), secs)
    }

    /// [`hold_for`](Self::hold_for) with the current time given, so a test
    /// moves time by passing later instants.
    fn hold_until(&self, now: Instant, secs: f64) -> Duration {
        let until = now + Duration::from_secs_f64(clamp_hold_secs(secs));
        let mut slot = locked(&self.until);
        let held = match *slot {
            Some(existing) if existing > until => existing,
            _ => until,
        };
        *slot = Some(held);
        held.saturating_duration_since(now)
    }

    /// What is left of the hold, or `None` when none is active.
    pub fn remaining(&self) -> Option<Duration> {
        self.remaining_at(Instant::now())
    }

    fn remaining_at(&self, now: Instant) -> Option<Duration> {
        let until = (*locked(&self.until))?;
        (until > now).then(|| until - now)
    }

    /// Waits (async) until no hold is active.
    pub async fn wait(&self) {
        while let Some(wait) = self.remaining() {
            sleep(wait).await;
        }
    }

    /// Like [`wait`](Self::wait), but only for a hold of at most `max`: when
    /// more than `max` is left (now, or after another 429 extended it while
    /// waiting) it returns `Err(what is left)` at once. R160: the `other`
    /// family's in-call wait is bounded so a 120 s hold cannot park a caller
    /// (the poll loop's live fetches among them) for minutes.
    pub async fn wait_up_to(&self, max: Duration) -> Result<(), Duration> {
        while let Some(left) = self.remaining() {
            if left > max {
                return Err(left);
            }
            sleep(left).await;
        }
        Ok(())
    }
}

/// A token bucket: `burst` tokens are available immediately, refilling at
/// `rps` tokens/sec up to `burst`.
#[derive(Debug)]
pub struct Throttle {
    burst: f64,
    rps: f64,
    inner: Mutex<Inner>,
    hold: Hold,
}

impl Throttle {
    /// `burst`/`rps` are clamped rather than rejected: a negative or `NaN`
    /// `burst` becomes `0.0`, and a `rps <= 0` (or `NaN`) becomes
    /// [`MIN_RPS`] — never a panic, even when called with nonsensical
    /// values (see [`MIN_RPS`]'s doc comment).
    pub fn new(burst: f64, rps: f64) -> Self {
        let burst = if burst >= 0.0 { burst } else { 0.0 };
        let rps = if rps > 0.0 { rps } else { MIN_RPS };
        Self {
            burst,
            rps,
            inner: Mutex::new(Inner {
                tokens: burst,
                last_refill: None,
            }),
            hold: Hold::new(),
        }
    }

    /// Waits (async) until a token is available — first waiting out any
    /// active hold, then waiting for the bucket to refill if it's empty —
    /// then consumes one token.
    pub async fn take(&self) {
        loop {
            if let Some(wait) = self.hold.remaining() {
                sleep(wait).await;
                continue;
            }
            match self.try_take_token() {
                Ok(()) => return,
                Err(wait) => sleep(wait).await,
            }
        }
    }

    /// Like [`take`](Self::take), but never waits out a hold: when one is
    /// active (or starts while this waits for a token) it returns
    /// `Err(what is left of it)` without consuming a token. The bucket's own
    /// refill wait (a few seconds at most) still happens in here. R160: an
    /// `/mmr/` call must not park the sync worker for a 60 s Riot block.
    pub async fn take_unless_held(&self) -> Result<(), Duration> {
        loop {
            if let Some(left) = self.hold.remaining() {
                return Err(left);
            }
            match self.try_take_token() {
                Ok(()) => return Ok(()),
                Err(wait) => sleep(wait).await,
            }
        }
    }

    /// Forces every `take()` to wait at least `secs` seconds from now,
    /// regardless of tokens on hand. Used after a `429` response, with
    /// `secs` taken from `Retry-After` (or a fallback backoff). Returns what
    /// is left of the hold (see [`Hold::hold_for`]).
    pub fn hold_for(&self, secs: f64) -> Duration {
        self.hold.hold_for(secs)
    }

    /// Refills the bucket for elapsed time, then either takes a token
    /// (`Ok`) or reports how long to wait for the next one (`Err`).
    fn try_take_token(&self) -> Result<(), Duration> {
        let mut inner = locked(&self.inner);
        let now = Instant::now();
        let elapsed = inner
            .last_refill
            .map(|last| now.duration_since(last).as_secs_f64())
            .unwrap_or(0.0);
        inner.last_refill = Some(now);
        inner.tokens = (inner.tokens + elapsed * self.rps).min(self.burst);

        if inner.tokens >= 1.0 {
            inner.tokens -= 1.0;
            Ok(())
        } else {
            let need = 1.0 - inner.tokens;
            Err(Duration::from_secs_f64(need / self.rps))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_hold_secs_maps_nan_to_zero() {
        assert_eq!(clamp_hold_secs(f64::NAN), 0.0);
    }

    #[test]
    fn clamp_hold_secs_maps_positive_infinity_to_the_max() {
        assert_eq!(clamp_hold_secs(f64::INFINITY), MAX_HOLD_SECS);
    }

    #[test]
    fn clamp_hold_secs_maps_negative_infinity_and_negatives_to_zero() {
        assert_eq!(clamp_hold_secs(f64::NEG_INFINITY), 0.0);
        assert_eq!(clamp_hold_secs(-5.0), 0.0);
    }

    #[test]
    fn the_ceiling_is_120s_so_a_60s_riot_block_is_held_in_full() {
        assert_eq!(MAX_HOLD_SECS, 120.0);
        assert_eq!(clamp_hold_secs(60.0), 60.0);
        assert_eq!(clamp_hold_secs(121.0), 120.0);
    }

    #[test]
    fn a_hold_is_extended_never_shrunk_and_reports_what_is_left() {
        let t0 = Instant::now();
        let h = Hold::new();
        assert_eq!(h.remaining_at(t0), None);
        assert_eq!(h.hold_until(t0, 60.0), Duration::from_secs(60));
        // A shorter one is a no-op; a longer one extends.
        assert_eq!(h.hold_until(t0, 5.0), Duration::from_secs(60));
        assert_eq!(
            h.remaining_at(t0 + Duration::from_secs(10)),
            Some(Duration::from_secs(50))
        );
        assert_eq!(h.hold_until(t0, 90.0), Duration::from_secs(90));
        assert_eq!(h.remaining_at(t0 + Duration::from_secs(90)), None);
    }

    #[test]
    fn a_nonsense_hold_is_no_hold() {
        let t0 = Instant::now();
        let h = Hold::new();
        assert_eq!(h.hold_until(t0, f64::NAN), Duration::ZERO);
        assert_eq!(h.remaining_at(t0), None);
    }

    #[test]
    fn clamp_hold_secs_clamps_a_huge_finite_value_to_the_max() {
        assert_eq!(clamp_hold_secs(1_000_000.0), MAX_HOLD_SECS);
    }

    /// Poisons `m` the way a panicking caller would: a thread dies holding it.
    fn poison<T: Send>(m: &Mutex<T>) {
        std::thread::scope(|s| {
            let _ = s
                .spawn(|| {
                    let _held = m.lock().unwrap();
                    panic!("a caller panicked while it held the lock");
                })
                .join();
        });
        assert!(m.is_poisoned());
    }

    #[tokio::test]
    async fn a_lock_poisoned_by_a_panicking_caller_does_not_stop_later_calls() {
        let t = Throttle::new(2.0, 1000.0);
        poison(&t.inner);
        poison(&t.hold.until);

        // Every path that takes either lock still answers.
        assert!(t.try_take_token().is_ok());
        t.take().await;
        assert_eq!(t.take_unless_held().await, Ok(()));
        assert!(t.hold.remaining().is_none());
        assert!(t.hold_for(30.0) > Duration::from_secs(29));
        assert!(t.hold.remaining().is_some());
        assert_eq!(
            t.take_unless_held()
                .await
                .map_err(|left| left.as_secs() >= 29),
            Err(true),
            "the hold set after the poisoning is honoured"
        );
    }

    #[test]
    fn clamp_hold_secs_passes_through_in_range_values() {
        assert_eq!(clamp_hold_secs(10.0), 10.0);
        assert_eq!(clamp_hold_secs(0.0), 0.0);
        assert_eq!(clamp_hold_secs(MAX_HOLD_SECS), MAX_HOLD_SECS);
    }
}
