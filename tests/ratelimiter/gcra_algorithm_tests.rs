// tests/FluxLimiter/gcra_algorithm_tests.rs

#[cfg(test)]
mod tests {

    use crate::fixtures::test_clock::TestClock;
    use flux_limiter::{FluxLimiter, FluxLimiterConfig};

    // GCRA algorithm tests
    #[test]
    fn first_request_always_allowed() {
        let clock = TestClock::new(0.0);
        let config = FluxLimiterConfig::new(1.0, 1.0);
        let limiter = FluxLimiter::with_config(config, clock).unwrap();
        let decision = limiter.check_request("client1").unwrap();
        assert!(decision.allowed);
    }

    #[test]
    fn rate_limiting_blocks_rapid_requests() {
        let clock = TestClock::new(0.0);
        let config = FluxLimiterConfig::new(1.0, 0.0); // 1 req/sec, no burst
        let limiter = FluxLimiter::with_config(config, clock.clone()).unwrap();
        let client = "client1";

        // First request at time 0.0 should be allowed
        let decision1 = limiter.check_request(client).unwrap();
        assert!(decision1.allowed);

        // Second request immediately after should be blocked
        let decision2 = limiter.check_request(client).unwrap();
        assert!(!decision2.allowed);

        // Request at 0.5 seconds should still be blocked
        clock.set_time(0.5);
        let decision3 = limiter.check_request(client).unwrap();
        assert!(!decision3.allowed);

        // Request at 1.0 seconds should be allowed (exactly 1 second later)
        clock.set_time(1.0);
        let decision4 = limiter.check_request(client).unwrap();
        assert!(decision4.allowed);

        // Another immediate request should be blocked again
        let decision5 = limiter.check_request(client).unwrap();
        assert!(!decision5.allowed);
    }

    #[test]
    fn burst_allowance_works() {
        let clock = TestClock::new(0.0);
        let config = FluxLimiterConfig::new(1.0, 3.0); // 1 req/sec, burst of 3
        let limiter = FluxLimiter::with_config(config, clock.clone()).unwrap();
        let client = "client1";

        // First 4 requests should all be allowed (burst capacity)
        assert!(limiter.check_request(client).unwrap().allowed);
        assert!(limiter.check_request(client).unwrap().allowed);
        assert!(limiter.check_request(client).unwrap().allowed);
        assert!(limiter.check_request(client).unwrap().allowed);

        // 5th request at same time should be blocked (burst exhausted)
        assert!(!limiter.check_request(client).unwrap().allowed);

        // After 1 second, 1 more request should be allowed
        clock.set_time(1.0);
        assert!(limiter.check_request(client).unwrap().allowed);

        // But immediate follow-up should be blocked
        assert!(!limiter.check_request(client).unwrap().allowed);
    }

    #[test]
    fn multiple_clients_independent() {
        let clock = TestClock::new(0.0);
        let config = FluxLimiterConfig::new(1.0, 0.0); // 1 req/sec, no burst
        let limiter = FluxLimiter::with_config(config, clock.clone()).unwrap();

        // Both clients' first requests should be allowed
        assert!(limiter.check_request("client1").unwrap().allowed);
        assert!(limiter.check_request("client2").unwrap().allowed);

        // Both clients' immediate second requests should be blocked
        assert!(!limiter.check_request("client1").unwrap().allowed);
        assert!(!limiter.check_request("client2").unwrap().allowed);

        // After 1 second, both should be allowed again
        clock.set_time(1.0);
        assert!(limiter.check_request("client1").unwrap().allowed);
        assert!(limiter.check_request("client2").unwrap().allowed);

        // Client1 exhausts their allowance, but client2 should still work
        assert!(!limiter.check_request("client1").unwrap().allowed);

        // Client3 (new client) should be allowed even though others are blocked
        assert!(limiter.check_request("client3").unwrap().allowed);
    }

    #[test]
    fn time_progression_allows_requests() {
        let clock = TestClock::new(0.0);
        let config = FluxLimiterConfig::new(2.0, 0.0); // 2 req/sec, no burst
        let limiter = FluxLimiter::with_config(config, clock.clone()).unwrap();
        let client = "client1";

        // First request at t=0 should be allowed
        assert!(limiter.check_request(client).unwrap().allowed);

        // Immediate second request should be blocked
        assert!(!limiter.check_request(client).unwrap().allowed);

        // Request at 0.25 seconds should still be blocked (need 0.5s interval for 2 req/sec)
        clock.set_time(0.25);
        assert!(!limiter.check_request(client).unwrap().allowed);

        // Request at exactly 0.5 seconds should be allowed
        clock.set_time(0.5);
        assert!(limiter.check_request(client).unwrap().allowed);

        // Immediate follow-up should be blocked again
        assert!(!limiter.check_request(client).unwrap().allowed);

        // Another 0.5 seconds later (t=1.0) should be allowed
        clock.set_time(1.0);
        assert!(limiter.check_request(client).unwrap().allowed);

        // Long idle period - request at t=10.0 should definitely be allowed
        clock.set_time(10.0);
        assert!(limiter.check_request(client).unwrap().allowed);
    }

    #[test]
    fn test_clock_advances_time() {
        let clock = TestClock::new(5.0);
        assert_eq!(clock.time_as_f64(), 5.0);

        clock.advance(2.5);
        assert_eq!(clock.time_as_f64(), 7.5);

        clock.set_time(0.0);
        assert_eq!(clock.time_as_f64(), 0.0);
    }

    #[test]
    fn accessor_methods_work() {
        let clock = TestClock::new(0.0);
        let config = FluxLimiterConfig::new(10.0, 5.0);
        let limiter = FluxLimiter::<String, _>::with_config(config, clock).unwrap();

        // Test that accessors return the original user-provided values
        assert_eq!(limiter.rate(), 10.0);
        assert_eq!(limiter.burst(), 5.0);
    }

    #[test]
    fn nanosecond_precision() {
        let clock = TestClock::new(0.0);
        let config = FluxLimiterConfig::new(1_000_000.0, 0.0); // 1M req/sec
        let limiter = FluxLimiter::with_config(config, clock.clone()).unwrap();
        let client = "client1";

        // First request should be allowed
        let decision = limiter.check_request(client).expect("Clock should work");
        assert!(decision.allowed);

        // Second request immediately should be blocked
        assert!(
            !limiter
                .check_request(client)
                .expect("Clock should work")
                .allowed
        );

        // Advance by exactly 1 microsecond (1000 nanoseconds)
        clock.advance(0.000001);
        assert!(
            limiter
                .check_request(client)
                .expect("Clock should work")
                .allowed
        );
    }
}
