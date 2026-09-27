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
