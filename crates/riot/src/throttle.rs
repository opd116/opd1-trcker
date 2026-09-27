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
    pub fn new(burst: f64, rps: f64) -> Self {
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
        let until = Instant::now() + Duration::from_secs_f64(secs.max(0.0));
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
