// tests/ratelimiter/clock_tests.rs
//
// Tests for MonotonicClock, the default clock, and the time semantics it gives
// the limiter. The backwards-clock-step tests inject a controllable time source
// so that "real elapsed time" and "what the OS clock says" can be moved
// independently; nothing here sleeps.

use flux_limiter::hidden::ElapsedSource;
use flux_limiter::{Clock, FluxLimiter, FluxLimiterConfig, MonotonicClock, SystemClock};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

const SECOND: u64 = 1_000_000_000;

/// A time source whose elapsed time and OS-clock reading are set by the test.
struct FakeSource {
    elapsed: AtomicU64,
    epoch: AtomicU64,
}

impl FakeSource {
    fn new(epoch: u64) -> Arc<Self> {
        Arc::new(Self {
            elapsed: AtomicU64::new(0),
            epoch: AtomicU64::new(epoch),
        })
    }
    fn advance(&self, nanos: u64) {
        self.elapsed.fetch_add(nanos, Ordering::SeqCst);
    }
    fn set_epoch(&self, nanos: u64) {
        self.epoch.store(nanos, Ordering::SeqCst);
    }
}

impl ElapsedSource for FakeSource {
    fn elapsed_nanos(&self) -> u64 {
        self.elapsed.load(Ordering::SeqCst)
    }
    fn epoch_nanos(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }
}

/// Clock + source pair, already anchored at `epoch`.
fn anchored(epoch: u64) -> (MonotonicClock, Arc<FakeSource>) {
    let source = FakeSource::new(epoch);
    let clock = MonotonicClock::with_source(source.clone());
    clock.now().unwrap(); // record the anchor now, so later steps are "after anchoring"
    (clock, source)
}

// ---------------------------------------------------------------------------
// The bug: a backwards OS-clock step
// ---------------------------------------------------------------------------

#[test]
fn backwards_os_clock_step_does_not_change_decisions() {
    // rate 1/s, burst 0: exactly one request per second, no credit to spare.
    let (clock, source) = anchored(100 * SECOND);
    let limiter = FluxLimiter::with_config(FluxLimiterConfig::new(1.0, 0.0), clock).unwrap();

    assert!(limiter.check_request(1u64).unwrap().allowed);

    // NTP steps the OS clock back 10 s. Real time has not moved.
    source.set_epoch(90 * SECOND);

    // Still denied: no real time has passed. This is correct, not an outage,
    // and retry_after reflects real time rather than the step.
    let d = limiter.check_request(1u64).unwrap();
    assert!(!d.allowed, "no real time elapsed, so this must be denied");
    let retry = d.retry_after_seconds.unwrap();
    assert!(
        (0.9..=1.0).contains(&retry),
        "retry_after must reflect real time (about 1 s), got {retry}"
    );
}

/// Makes one request per real second for `seconds` seconds, with the OS clock
/// stepped back by 10 s after the first request if `step` is set. Returns how
/// many of those requests were allowed.
fn one_per_second(seconds: u64, step: bool) -> usize {
    let source = FakeSource::new(100 * SECOND);
    let clock = MonotonicClock::with_source(source.clone());
    let limiter = FluxLimiter::with_config(FluxLimiterConfig::new(1.0, 1.0), clock).unwrap();
    let mut allowed = 0;
    for i in 0..seconds {
        if i == 1 && step {
            source.set_epoch(90 * SECOND);
        }
        source.advance(SECOND);
        if limiter.check_request(1u64).unwrap().allowed {
            allowed += 1;
        }
    }
    allowed
}

#[test]
fn refills_by_real_elapsed_time_while_os_clock_is_behind() {
    // One request every real second must all be allowed, whether or not the OS
    // clock has been stepped back. This is the property the clamping
    // workaround lacks: with it, every request is denied until the OS clock
    // climbs back past the anchor.
    let unstepped = one_per_second(10, false);
    let stepped = one_per_second(10, true);
    assert_eq!(unstepped, 10, "1/s for 10 s with burst 1 allows all 10");
    assert_eq!(
        stepped, unstepped,
        "an OS clock step must not change what a client is allowed"
    );
}

#[test]
fn forward_os_clock_step_is_invisible() {
    let (clock, source) = anchored(100 * SECOND);
    let limiter = FluxLimiter::with_config(FluxLimiterConfig::new(1.0, 0.0), clock).unwrap();

    assert!(limiter.check_request(1u64).unwrap().allowed);

    // OS clock jumps forward one hour; real time has not moved.
    source.set_epoch(100 * SECOND + 3600 * SECOND);

    // Capacity must NOT refill: no real time elapsed. With SystemClock a one-hour
    // forward step refills everything; this clock never sees the step.
    assert!(
        !limiter.check_request(1u64).unwrap().allowed,
        "a forward OS clock step must not refill capacity"
    );
}

#[test]
fn readings_never_go_backwards_across_a_step() {
    let (clock, source) = anchored(100 * SECOND);
    let a = clock.now().unwrap();
    source.set_epoch(SECOND); // a 99 s backwards step
    source.advance(SECOND / 2);
    let b = clock.now().unwrap();
    assert!(
        b > a,
        "reading after a backwards step must still advance: {a} -> {b}"
    );
}

// ---------------------------------------------------------------------------
// Anchoring and reset_time_nanos
// ---------------------------------------------------------------------------

#[test]
fn reset_time_tracks_the_epoch_not_the_stepped_clock() {
    let (clock, source) = anchored(100 * SECOND);
    let d = FluxLimiter::with_config(FluxLimiterConfig::new(1.0, 1.0), clock)
        .unwrap()
        .check_request(1u64)
        .unwrap();
    source.set_epoch(0); // never read again after anchoring
    // TAT = now + 1 s = 101 s after the epoch, in epoch nanos.
    assert_eq!(d.reset_time_nanos, 101 * SECOND);
}

#[test]
fn reset_time_is_within_one_second_of_the_real_epoch() {
    let clock = MonotonicClock::new(); // the real clock
    let limiter = FluxLimiter::with_config(FluxLimiterConfig::new(1.0, 1.0), clock).unwrap();
    let d = limiter.check_request(1u64).unwrap();
    let real_epoch = SystemClock.now().unwrap();
    let diff = d.reset_time_nanos.abs_diff(real_epoch);
    assert!(
        diff <= 2 * SECOND,
        "reset_time_nanos should be ~1 s ahead of the real epoch, off by {diff} ns"
    );
}

#[test]
fn readings_are_monotonic_per_thread_under_contention() {
    // Per-thread comparison only: comparing against a global maximum reports
    // false failures whenever a thread is descheduled between read and compare.
    let backwards = AtomicU64::new(0);
    std::thread::scope(|s| {
        for _ in 0..8 {
            s.spawn(|| {
                let clock = MonotonicClock::new();
                let mut prev = 0u64;
                for _ in 0..20_000 {
                    let n = clock.now().unwrap();
                    if n < prev {
                        backwards.fetch_add(1, Ordering::Relaxed);
                    }
                    prev = n;
                }
            });
        }
    });
    assert_eq!(backwards.load(Ordering::Relaxed), 0, "clock went backwards");
}

#[test]
fn clones_share_one_anchor() {
    let a = MonotonicClock::new();
    let b = a.clone();
    a.now().unwrap();
    b.now().unwrap();
    assert_eq!(a.anchor_id(), b.anchor_id());
    // and they agree, since they share the anchor and the source
    assert!(a.now().unwrap().abs_diff(b.now().unwrap()) < SECOND);
}

#[test]
fn cloned_limiter_sees_the_same_time_as_its_original() {
    // FluxLimiter::clone shares client state; its clock must not re-anchor, or the
    // two would disagree about 'now' while sharing TATs.
    let (clock, source) = anchored(100 * SECOND);
    let limiter = FluxLimiter::with_config(FluxLimiterConfig::new(1.0, 0.0), clock).unwrap();
    let clone = limiter.clone();
    assert!(limiter.check_request(1u64).unwrap().allowed);

    // 0.5 s of real time: neither may allow a second request.
    source.advance(SECOND / 2);
    assert!(
        !limiter.check_request(1u64).unwrap().allowed,
        "only 0.5 s elapsed at 1/s"
    );
    assert!(
        !clone.check_request(1u64).unwrap().allowed,
        "clone must see the same 0.5 s"
    );

    // 1.0 s total: both allow exactly one, and they share that allowance.
    source.advance(SECOND / 2);
    let a = limiter.check_request(1u64).unwrap().allowed;
    let b = clone.check_request(1u64).unwrap().allowed;
    assert_ne!(
        a, b,
        "shared state: exactly one of the two gets the request at t=1 s"
    );
}

#[test]
fn independent_clocks_agree_to_within_one_second() {
    let a = MonotonicClock::new();
    let b = MonotonicClock::default();
    let x = a.now().unwrap();
    let y = b.now().unwrap();
    assert!(
        x.abs_diff(y) < SECOND,
        "independent anchors disagreed: {x} vs {y}"
    );
}

// ---------------------------------------------------------------------------
// Type-level guarantees
// ---------------------------------------------------------------------------

/* TEMP(task 2): enable once the default type parameter is switched
fn default_clock_parameter_is_monotonic() {
    // No clock argument anywhere: this is `FluxLimiter<u64, MonotonicClock>`.
    // No clock *type* is named anywhere: this is `FluxLimiter<u64, MonotonicClock>`.
    let limiter: FluxLimiter<u64> =
        FluxLimiter::with_config(FluxLimiterConfig::new(1.0, 1.0), MonotonicClock::new()).unwrap();
    assert!(limiter.check_request(1u64).unwrap().allowed);

    fn assert_monotonic<C: flux_limiter::Clock>(_: &FluxLimiter<u64, C>) {}
    fn assert_is_monotonic(_: &FluxLimiter<u64, MonotonicClock>) {}
    assert_is_monotonic(&limiter);
    assert_monotonic(&limiter);
}
*/

#[test]
fn system_clock_still_exists_and_implements_clock() {
    fn impls_clock<C: Clock>(_: &C) {}
    impls_clock(&SystemClock);
    impls_clock(&MonotonicClock::new());
    let n = SystemClock.now().unwrap();
    assert!(
        n > 1_500_000_000 * SECOND,
        "SystemClock should be epoch nanos, got {n}"
    );
}

#[test]
fn monotonic_clock_never_returns_an_error() {
    let clock = MonotonicClock::new();
    for _ in 0..100 {
        assert!(clock.now().is_ok());
    }
}
