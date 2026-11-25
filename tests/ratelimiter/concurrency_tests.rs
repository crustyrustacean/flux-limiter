// tests/ratelimiter/concurrency_tests.rs

// Concurrency tests for the lock-free per-client path.
//
// These exercise the property the atomic TAT exists to guarantee: no matter how
// many requests for one client race, exactly as many are allowed as would be
// allowed if the same requests arrived one at a time. A lost update between two
// concurrent requests would over-allow; these tests are written so that such a
// bug fails them.
//
// The expected count is `burst + 1`, which is the limiter's existing convention
// rather than an off-by-one here: a client's first request is granted
// unconditionally, and then `burst` more fit within the tolerance.
//
// Counting allowed requests alone is a weak check for lost updates: once the
// burst is spent every request is denied and takes a path that writes nothing, so
// a race only gets `burst + 1` chances to matter. The TAT test below instead uses a
// burst large enough that every request writes, so a single lost update is
// visible as a final TAT that is short by exactly one interval.

use std::sync::Arc;
use std::sync::Barrier;
use std::thread;

use flux_limiter::{FluxLimiter, FluxLimiterConfig};

use crate::fixtures::test_clock::TestClock;

/// A limiter over a clock frozen at time 0, so the passage of time can never
/// refill a burst during a test. Every allowed request is therefore attributed to
/// contention, not to elapsed time.
fn frozen_limiter(rate: f64, burst: f64) -> FluxLimiter<String, TestClock> {
    FluxLimiter::with_config(FluxLimiterConfig::new(rate, burst), TestClock::new(0.0)).unwrap()
}

/// Spawns `threads` OS threads that all hit `client` at once, `per_thread` times
/// each, against a frozen clock. Returns how many requests were allowed.
///
/// A barrier makes every thread call `check_request` at the same instant, which is
/// the condition under which a read-check-write race would lose an update.
fn hammer_one_client(threads: usize, per_thread: usize, rate: f64, burst: f64) -> usize {
    let limiter = Arc::new(frozen_limiter(rate, burst));
    let barrier = Arc::new(Barrier::new(threads));
    let mut handles = Vec::with_capacity(threads);

    for _ in 0..threads {
        let limiter = Arc::clone(&limiter);
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            barrier.wait();
            let mut allowed = 0usize;
            for i in 0..per_thread {
                // Every thread targets the same key, so every call contends.
                let decision = limiter.check_request("shared-client".to_string()).unwrap();
                allowed += usize::from(decision.allowed);
                black_box(i);
            }
            allowed
        }));
    }

    handles.into_iter().map(|h| h.join().unwrap()).sum()
}

#[inline(never)]
fn black_box<T>(value: T) -> T {
    value
}

#[test]
fn concurrent_requests_never_exceed_burst_for_one_client() {
    // 16 threads x 25 requests = 400 simultaneous attempts against a burst of 10.
    // Exactly burst + 1 = 11 may be allowed, the same count a sequential run
    // gives. A lost update would let two requests both see the same TAT and both
    // be allowed, pushing the count above 11.
    for _ in 0..200 {
        let allowed = hammer_one_client(16, 25, 1.0, 10.0);
        assert_eq!(
            allowed, 11,
            "concurrent requests over-allowed: burst is 10 but {allowed} passed"
        );
    }
}

#[test]
fn concurrent_first_requests_for_a_new_client_allow_exactly_the_burst() {
    // Targets the insert path specifically: the client does not exist yet, so
    // every thread races to insert it. A loser of that race that treated its own
    // request as a fresh "first request" would grant a new allowance each time.
    // Expected is burst + 1, the same as a sequential run.
    for _ in 0..200 {
        let allowed = hammer_one_client(16, 5, 1.0, 3.0);
        assert_eq!(
            allowed, 4,
            "insert race granted extra first-request allowance: {allowed} of burst 3"
        );
    }
}

#[test]
fn concurrent_requests_with_burst_of_one_allow_exactly_one() {
    // The tightest bound that still leaves room to cheat: a burst of 1 allows 2
    // (the first request plus one), so a single lost update pushes the count to
    // 3 and fails this.
    for _ in 0..200 {
        let allowed = hammer_one_client(16, 10, 1.0, 1.0);
        assert_eq!(
            allowed, 2,
            "burst of 1 allowed {allowed} concurrent requests"
        );
    }
}

/// The strongest check for lost updates. With a burst far larger than the number
/// of requests, every request is allowed and every one must advance the TAT by
/// exactly one interval. Two requests that both observed the same TAT and both
/// wrote would leave the final TAT one interval short.
#[test]
fn every_concurrent_write_lands_in_the_tat() {
    const THREADS: usize = 16;
    const PER_THREAD: usize = 1_000;
    const RATE_NANOS: u64 = 1_000_000_000;

    for _ in 0..50 {
        let limiter = Arc::new(frozen_limiter(1.0, 1e9));
        let barrier = Arc::new(Barrier::new(THREADS));

        let last_reset: Vec<u64> = thread::scope(|s| {
            let handles: Vec<_> = (0..THREADS)
                .map(|_| {
                    let limiter = Arc::clone(&limiter);
                    let barrier = Arc::clone(&barrier);
                    s.spawn(move || {
                        barrier.wait();
                        let mut last = 0;
                        for _ in 0..PER_THREAD {
                            last = limiter
                                .check_request("shared-client".to_string())
                                .unwrap()
                                .reset_time_nanos;
                        }
                        last
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });

        // The final TAT is whichever thread wrote last, so it is the maximum of
        // the last value each thread observed.
        let final_tat = last_reset.into_iter().max().unwrap();
        let expected = (THREADS * PER_THREAD) as u64 * RATE_NANOS;
        assert_eq!(
            final_tat,
            expected,
            "lost updates: final TAT is {} intervals short",
            (expected - final_tat) / RATE_NANOS
        );
    }
}

#[test]
fn concurrent_requests_across_many_clients_each_respect_their_own_burst() {
    // Different clients must not interfere: the atomic for one client is
    // independent of every other, so a correct run allows (burst + 1) per client.
    let clients = 64;
    let limiter = Arc::new(frozen_limiter(1.0, 4.0));
    let barrier = Arc::new(Barrier::new(16));

    let allowed: usize = thread::scope(|s| {
        let handles: Vec<_> = (0..16)
            .map(|_| {
                let limiter = Arc::clone(&limiter);
                let barrier = Arc::clone(&barrier);
                s.spawn(move || {
                    barrier.wait();
                    let mut allowed = 0usize;
                    for i in 0..100 {
                        let client = format!("client-{}", i % clients);
                        let decision = limiter.check_request(client).unwrap();
                        allowed += usize::from(decision.allowed);
                    }
                    allowed
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).sum()
    });

    // Each client allows burst + 1, like a sequential run.
    assert_eq!(
        allowed,
        (4 + 1) * clients,
        "per-client bursts interfered with each other"
    );
}
