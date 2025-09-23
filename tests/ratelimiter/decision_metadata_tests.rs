// tests/ratelimiter/decision_metadata_tests.rs

#[cfg(test)]
mod tests {

    use crate::fixtures::test_clock::TestClock;
    use flux_limiter::{FluxLimiter, FluxLimiterConfig};

    #[test]
    fn check_request_returns_detailed_decision() {
        let clock = TestClock::new(0.0);
        let config = FluxLimiterConfig::new(1.0, 2.0); // 1 req/sec, burst of 2
        let limiter = FluxLimiter::with_config(config, clock.clone()).unwrap();
        let client = "client1";

        // First request should be allowed with metadata
        let decision = limiter.check_request(client).unwrap();
        assert!(decision.allowed);
        assert!(decision.retry_after_seconds.is_none());
        assert!(decision.remaining_capacity.is_some());
        assert!(decision.reset_time_nanos > 0);
    }
}
