//! A token-bucket rate limiter with a `hold_for` escape hatch for `429
//! Retry-After` responses.
//!
//! Mirrors Valorant-Scout's `backend/riot_client.py` throttle:
//! `_take_token` (burst + linear refill at `rps` tokens/sec, blocking the
//! caller until a token is free) and `_set_hold`/`held_secs` (a hard floor
//! on the next `take()`, extended — never shrunk — by repeated holds).

use std::sync::Mutex;
use std::time::{Duration, Instant};

use tokio::time::sleep;

/// The smallest `rps` a [`Throttle`] will ever actually use. Guards
/// against a nonsensical `rps <= 0` (or NaN) causing a divide-by-zero (or
/// by-a-negative-number) when computing how long to wait for the next
/// token — which would hand a non-finite or negative value to
/// `Duration::from_secs_f64`, and panic. Release builds use
/// `panic = "abort"`, so a panic here would take the whole app down. Real
/// call sites always pass a real positive `rps` (10.0, 0.4); this only
/// protects against misuse.
const MIN_RPS: f64 = 0.01;

/// The longest a single `hold_for` call will ever actually hold,
/// regardless of what it's asked for. Guards against a malformed or
/// absurd caller value (`f64::INFINITY`, `f64::NAN`, a huge number — e.g.
/// from a bogus `Retry-After` header) turning into a `Duration`/`Instant`
/// that panics, or an effectively-infinite hold.
const MAX_HOLD_SECS: f64 = 30.0;

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
    /// The instant before which every `take()` must wait, regardless of
    /// tokens on hand. Set by `hold_for()` on a `429`.
    hold_until: Option<Instant>,
}

/// A token bucket: `burst` tokens are available immediately, refilling at
/// `rps` tokens/sec up to `burst`.
#[derive(Debug)]
pub struct Throttle {
    burst: f64,
    rps: f64,
    inner: Mutex<Inner>,
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
                hold_until: None,
            }),
        }
    }

    /// Waits (async) until a token is available — first waiting out any
    /// active hold, then waiting for the bucket to refill if it's empty —
    /// then consumes one token.
    pub async fn take(&self) {
        loop {
            if let Some(wait) = self.hold_wait() {
                sleep(wait).await;
                continue;
            }
            match self.try_take_token() {
                Ok(()) => return,
                Err(wait) => sleep(wait).await,
            }
        }
    }

    /// Forces every `take()` to wait at least `secs` seconds from now,
    /// regardless of tokens on hand. Used after a `429` response, with
    /// `secs` taken from `Retry-After` (or a fallback backoff).
    ///
    /// A shorter hold never shrinks an already-active longer one — matches
    /// Scout's `_HOLD_UNTIL[fam] = max(_HOLD_UNTIL[fam], now + seconds)`.
    pub fn hold_for(&self, secs: f64) {
        let until = Instant::now() + Duration::from_secs_f64(clamp_hold_secs(secs));
        let mut inner = self.inner.lock().unwrap();
        inner.hold_until = Some(match inner.hold_until {
            Some(existing) if existing > until => existing,
            _ => until,
        });
    }

    fn hold_wait(&self) -> Option<Duration> {
        let inner = self.inner.lock().unwrap();
        let until = inner.hold_until?;
        let now = Instant::now();
        (until > now).then(|| until - now)
    }

    /// Refills the bucket for elapsed time, then either takes a token
    /// (`Ok`) or reports how long to wait for the next one (`Err`).
    fn try_take_token(&self) -> Result<(), Duration> {
        let mut inner = self.inner.lock().unwrap();
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
    fn clamp_hold_secs_clamps_a_huge_finite_value_to_the_max() {
        assert_eq!(clamp_hold_secs(1_000_000.0), MAX_HOLD_SECS);
    }

    #[test]
    fn clamp_hold_secs_passes_through_in_range_values() {
        assert_eq!(clamp_hold_secs(10.0), 10.0);
        assert_eq!(clamp_hold_secs(0.0), 0.0);
        assert_eq!(clamp_hold_secs(MAX_HOLD_SECS), MAX_HOLD_SECS);
    }
}
