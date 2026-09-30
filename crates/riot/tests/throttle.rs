//! Unit tests for the token-bucket [`Throttle`], matching Valorant-Scout's
//! `backend/riot_client.py` throttle semantics (burst + refill rate, plus a
//! `hold_for` escape hatch driven by `429 Retry-After`).
//!
//! Timing assertions use generous slack — we assert lower bounds only, per
//! the task brief ("throttle tests are timing-based: assert lower bounds
//! with slack, not exact times").

use std::time::{Duration, Instant};

use opd1_riot::throttle::Throttle;

#[tokio::test]
async fn throttle_limits_rate() {
    // Burst 10 / 10 rps: the first 10 takes are free, the next 10 must be
    // paced at 10/s, so 20 takes should take at least ~1.0s. Scout's own
    // `_RIOT_BUCKET` uses these exact values for the global bucket.
    let throttle = Throttle::new(10.0, 10.0);

    let start = Instant::now();
    for _ in 0..20 {
        throttle.take().await;
    }
    let elapsed = start.elapsed();

    assert!(
        elapsed >= Duration::from_millis(900),
        "expected >= 0.9s for 20 takes at burst 10 / 10rps, got {elapsed:?}"
    );
}

#[tokio::test]
async fn throttle_takes_burst_immediately() {
    // A throttle that has never been used should let `burst` takes through
    // essentially instantly (no artificial floor delay).
    let throttle = Throttle::new(10.0, 10.0);

    let start = Instant::now();
    for _ in 0..10 {
        throttle.take().await;
    }
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_millis(200),
        "expected the initial burst to be near-instant, got {elapsed:?}"
    );
}

#[tokio::test]
async fn throttle_hold_blocks() {
    // A throttle with a huge burst/rps would never block on tokens alone,
    // so any observed delay here comes from `hold_for`.
    let throttle = Throttle::new(1000.0, 1000.0);
    throttle.hold_for(0.3);

    let start = Instant::now();
    throttle.take().await;
    let elapsed = start.elapsed();

    assert!(
        elapsed >= Duration::from_millis(250),
        "expected hold_for(0.3) to block take() for ~0.3s, got {elapsed:?}"
    );
}

#[tokio::test]
async fn throttle_hold_does_not_extend_backwards() {
    // A shorter hold_for() call must not shrink an existing longer hold
    // (matches Scout's `max(_HOLD_UNTIL[fam], now + seconds)`).
    let throttle = Throttle::new(1000.0, 1000.0);
    throttle.hold_for(0.3);
    throttle.hold_for(0.05); // should be a no-op: 0.3s hold already active

    let start = Instant::now();
    throttle.take().await;
    let elapsed = start.elapsed();

    assert!(
        elapsed >= Duration::from_millis(250),
        "a shorter hold_for should not shrink the active hold, got {elapsed:?}"
    );
}

#[tokio::test]
async fn throttle_take_after_hold_expires_is_fast() {
    let throttle = Throttle::new(1000.0, 1000.0);
    throttle.hold_for(0.05);

    // Sleep past the hold, then take() should return immediately (plenty
    // of tokens in a 1000/1000 bucket).
    tokio::time::sleep(Duration::from_millis(80)).await;

    let start = Instant::now();
    throttle.take().await;
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_millis(50),
        "take() after the hold expired should be near-instant, got {elapsed:?}"
    );
}

#[tokio::test]
async fn hold_for_never_panics_on_malformed_seconds() {
    // A malformed/huge `Retry-After` (or any other bogus caller value)
    // must never reach `Duration::from_secs_f64`/`Instant + Duration`
    // unclamped — those panic on non-finite or out-of-range input, and
    // release builds unwind (R138), which would still end the calling
    // task. This test's assertion *is*
    // that none of these calls panic (a huge/infinite value clamps to a
    // real 30s hold internally, per `clamp_hold_secs`'s unit tests in
    // `src/throttle.rs` — we don't wait it out here, just prove
    // constructing it doesn't blow up).
    let throttle = Throttle::new(1000.0, 1000.0);
    throttle.hold_for(f64::INFINITY);
    throttle.hold_for(f64::NAN);
    throttle.hold_for(-5.0);
    throttle.hold_for(1_000_000.0);

    // NaN/negative hold_for calls above are no-ops (clamp to 0s and never
    // exceed the already-active hold), so a *separate*, never-held
    // throttle should still take its burst tokens immediately — proving
    // the throttle type as a whole stays in a normal, usable state.
    let fresh = Throttle::new(5.0, 5.0);
    let start = Instant::now();
    fresh.take().await;
    assert!(start.elapsed() < Duration::from_millis(50));
}

#[tokio::test]
async fn new_with_non_positive_or_nan_rps_does_not_panic() {
    // `rps <= 0` divides-by-zero (or by a negative number) when computing
    // how long to wait for the next token, which used to panic. Burst
    // tokens must still be handed out immediately regardless.
    for bad_rps in [0.0, -1.0, f64::NAN, f64::NEG_INFINITY] {
        let throttle = Throttle::new(5.0, bad_rps);
        let start = Instant::now();
        for _ in 0..5 {
            throttle.take().await;
        }
        assert!(
            start.elapsed() < Duration::from_millis(200),
            "burst tokens should still be handed out immediately for rps={bad_rps}"
        );
    }
}

#[test]
fn new_with_negative_or_nan_burst_does_not_panic() {
    // Construction alone (no `take()`/`hold_for()`) must never panic,
    // regardless of how nonsensical the arguments are.
    let _ = Throttle::new(-5.0, 10.0);
    let _ = Throttle::new(f64::NAN, 10.0);
}
