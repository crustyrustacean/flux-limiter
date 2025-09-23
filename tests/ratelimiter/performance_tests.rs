// tests/FluxLimiter/performance_tests.rs

#[cfg(test)]
mod tests {

    use crate::fixtures::test_clock::TestClock;
    use flux_limiter::{FluxLimiter, FluxLimiterConfig};

    #[test]
    fn retry_after_calculation_works() {
        let clock = TestClock::new(0.0);
        let config = FluxLimiterConfig::new(2.0, 0.0); // 2 req/sec, no burst
        let limiter = FluxLimiter::with_config(config, clock.clone()).unwrap();
        let client = "client1";

        // First request allowed
        assert!(limiter.check_request(client).unwrap().allowed);

        // Second request blocked, should suggest ~0.5 second retry
        let decision = limiter.check_request(client).unwrap();
        assert!(!decision.allowed);
        let retry_after = decision.retry_after_seconds.unwrap();
        assert!(retry_after > 0.4 && retry_after < 0.6); // Approximately 0.5 seconds
    }

    #[test]
    fn remaining_capacity_tracks_burst() {
        let clock = TestClock::new(0.0);
        let config = FluxLimiterConfig::new(1.0, 3.0); // 1 req/sec, burst of 3
        let limiter = FluxLimiter::with_config(config, clock.clone()).unwrap();
        let client = "client1";

        // First request should show remaining capacity
        let decision1 = limiter.check_request(client).unwrap();
        assert!(decision1.allowed);
        let remaining1 = decision1.remaining_capacity.unwrap();

        // Second request should show lower remaining capacity
        let decision2 = limiter.check_request(client).unwrap();
        assert!(decision2.allowed);
        let remaining2 = decision2.remaining_capacity.unwrap();

        assert!(remaining2 < remaining1);

        // Eventually we should hit zero remaining capacity
        limiter.check_request(client).unwrap(); // 3rd request
        limiter.check_request(client).unwrap(); // 4th request

        let blocked_decision = limiter.check_request(client).unwrap();
        assert!(!blocked_decision.allowed);
        assert_eq!(blocked_decision.remaining_capacity, Some(0.0));
    }
}
