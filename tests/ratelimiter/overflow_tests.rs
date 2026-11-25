// tests/ratelimiter/overflow_tests.rs
//
// Exercises `check_request` and `cleanup_stale_clients` with clock readings at
// the extremes of `u64`, on configurations that validation accepts. None of
// these may panic in a debug build or wrap in a release build.

#[cfg(test)]
mod tests {
    use crate::fixtures::test_clock::TestClock;
    use flux_limiter::{FluxLimiter, FluxLimiterConfig};

    /// A rate of 1e-3/s gives a 1e12 ns interval, the largest realistic one,
    /// so `now + interval` is the addition most likely to overflow.
    fn limiter_with_wide_interval(now: u64) -> (FluxLimiter<String, TestClock>, TestClock) {
        let clock = TestClock::new(0.0);
        clock.set_nanos(now);
        let limiter =
            FluxLimiter::with_config(FluxLimiterConfig::new(1e-3, 1_000_000.0), clock.clone())
                .expect("valid config");
        (limiter, clock)
    }

    #[test]
    fn first_request_at_clock_zero_does_not_overflow() {
        let (limiter, _) = limiter_with_wide_interval(0);
        let decision = limiter.check_request("a".to_string()).expect("clock works");
        assert!(decision.allowed);
        assert_eq!(decision.reset_time_nanos, 1_000_000_000_000);
    }

    #[test]
    fn first_request_at_a_current_epoch_value_does_not_overflow() {
        let now = 1_795_000_000_000_000_000; // ~Nov 2026 in epoch nanos
        let (limiter, _) = limiter_with_wide_interval(now);
        let decision = limiter.check_request("a".to_string()).expect("clock works");
        assert!(decision.allowed);
        assert_eq!(decision.reset_time_nanos, now + 1_000_000_000_000);
    }

    #[test]
    fn first_request_near_u64_max_saturates_instead_of_overflowing() {
        // The interval no longer fits above this reading, so the TAT must
        // saturate at u64::MAX rather than wrap to a small value in the past.
        let now = u64::MAX - 1;
        let (limiter, _) = limiter_with_wide_interval(now);
        let decision = limiter.check_request("a".to_string()).expect("clock works");
        assert!(decision.allowed);
        assert_eq!(decision.reset_time_nanos, u64::MAX);
    }

    #[test]
    fn request_at_exact_u64_max_does_not_overflow() {
        let (limiter, _) = limiter_with_wide_interval(u64::MAX);
        let decision = limiter.check_request("a".to_string()).expect("clock works");
        assert!(decision.allowed);
        assert_eq!(decision.reset_time_nanos, u64::MAX);
    }

    #[test]
    fn allowed_path_saturates_when_previous_tat_is_already_max() {
        let now = u64::MAX - 1;
        let (limiter, _) = limiter_with_wide_interval(now);
        let _ = limiter.check_request("a".to_string()).unwrap(); // TAT -> u64::MAX

        // A second request takes the occupied/allowed path with a TAT of u64::MAX.
        let decision = limiter.check_request("a".to_string()).expect("clock works");
        // Whether it conforms or not, it must not overflow, and the TAT must stay maxed.
        assert_eq!(decision.reset_time_nanos, u64::MAX);
    }

    #[test]
    fn cleanup_near_u64_max_does_not_overflow() {
        let (limiter, clock) = limiter_with_wide_interval(0);
        let _ = limiter.check_request("a".to_string()).unwrap();

        // tolerance = 1e6 * 1e12 = 1e18, so tat + tolerance is ~1e18 while now is
        // ~1.8e19: the client is legitimately stale (its TAT is ancient next to a
        // clock reading at the end of time). The point is that this is a clean
        // `Ok`, not an overflow.
        clock.set_nanos(u64::MAX - 5);
        limiter
            .cleanup_stale_clients(0)
            .expect("cleanup must not overflow");
        assert_eq!(limiter.client_count(), 0);
    }

    #[test]
    fn cleanup_keeps_a_client_whose_tolerance_would_have_overflowed() {
        // The case the saturating add exists for: a tolerance large enough that
        // tat + tolerance exceeds u64::MAX. With a plain '+' this wrapped to a
        // small number and the live client was evicted; saturating keeps it.
        let clock = TestClock::new(0.0);
        // rate = 1 -> T = 1e9. burst = 1.8e10 -> tolerance = 1.8e19, just under 2^64.
        let limiter = FluxLimiter::with_config(FluxLimiterConfig::new(1.0, 1.8e10), clock.clone())
            .expect("valid config");
        let _ = limiter.check_request("a".to_string()).unwrap();

        // now is small, so tat + tolerance (1.8e19 + 1e9) overflows u64.
        clock.set_nanos(1_000_000_000);
        limiter
            .cleanup_stale_clients(0)
            .expect("cleanup must not overflow");
        assert_eq!(
            limiter.client_count(),
            1,
            "a client with an overflowing tolerance must be kept, not evicted"
        );
    }

    #[test]
    fn cleanup_at_exact_u64_max_does_not_overflow() {
        let (limiter, clock) = limiter_with_wide_interval(0);
        let _ = limiter.check_request("a".to_string()).unwrap();
        clock.set_nanos(u64::MAX);
        limiter
            .cleanup_stale_clients(0)
            .expect("cleanup must not overflow");
    }

    #[test]
    fn smallest_accepted_rate_makes_a_first_request_without_overflow() {
        // The interval is just under 2^64; at a current epoch value the TAT
        // saturates, and at zero it does not.
        let extreme_rate = 1e9 / (u64::MAX as f64) * 2.0; // interval ~= 2^63 ns
        let clock = TestClock::new(0.0);
        let limiter =
            FluxLimiter::with_config(FluxLimiterConfig::new(extreme_rate, 0.0), clock.clone())
                .expect("valid config");

        clock.set_nanos(0);
        let d = limiter.check_request("a".to_string()).unwrap();
        assert!(d.allowed);

        clock.set_nanos(u64::MAX);
        // Denied (too soon) or allowed, but never a panic, and never a wrapped TAT.
        let d = limiter.check_request("a".to_string()).unwrap();
        assert_eq!(d.reset_time_nanos, u64::MAX);
    }
}
