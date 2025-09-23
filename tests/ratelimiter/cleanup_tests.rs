// tests/ratelimiter/gcra_algorithm_tests.rs

#[cfg(test)]
mod tests {

    use crate::fixtures::test_clock::TestClock;
    use flux_limiter::{FluxLimiter, FluxLimiterConfig};

    #[test]
    fn cleanup_removes_stale_clients() {
        let clock = TestClock::new(0.0);
        let config = FluxLimiterConfig::new(1.0, 0.0);
        let limiter = FluxLimiter::with_config(config, clock.clone()).unwrap();

        // Add some clients at different times
        assert!(
            limiter
                .check_request("client1".to_string())
                .unwrap()
                .allowed
        ); // TAT = t=1

        clock.set_time(5.0);
        assert!(
            limiter
                .check_request("client2".to_string())
                .unwrap()
                .allowed
        ); // TAT = t=6

        clock.set_time(10.0);
        assert!(
            limiter
                .check_request("client3".to_string())
                .unwrap()
                .allowed
        ); // TAT = t=11

        // Verify all clients are in the map
        assert_eq!(limiter.client_state.len(), 3);

        // Clean up clients older than 4.5 seconds at t=12
        // Cutoff will be 12 - 4.5 = 7.5, so keep TATs > 7.5
        clock.set_time(12.0);
        let threshold_nanos = (4.5 * 1_000_000_000.0) as u64;
        limiter.cleanup_stale_clients(threshold_nanos);

        // Only client3 (TAT=11) should remain
        assert_eq!(limiter.client_state.len(), 1);
        assert!(!limiter.client_state.contains_key("client1"));
        assert!(!limiter.client_state.contains_key("client2"));
        assert!(limiter.client_state.contains_key("client3"));

        // Clean up all remaining clients
        limiter.cleanup_stale_clients(0);
        assert_eq!(limiter.client_state.len(), 0);
    }

    #[test]
    fn cleanup_handles_empty_state() {
        let clock = TestClock::new(0.0);
        let config = FluxLimiterConfig::new(1.0, 0.0);
        let limiter = FluxLimiter::<String, _>::with_config(config, clock).unwrap();

        // Cleanup on empty state should not panic
        limiter.cleanup_stale_clients(1000);
        assert_eq!(limiter.client_state.len(), 0);
    }

    #[test]
    fn cleanup_preserves_recent_clients() {
        let clock = TestClock::new(100.0);
        let config = FluxLimiterConfig::new(10.0, 0.0);
        let limiter = FluxLimiter::with_config(config, clock.clone()).unwrap();

        // Add several recent clients
        for i in 0..5 {
            let client = format!("client{}", i);
            assert!(limiter.check_request(client).unwrap().allowed); // Pass owned String
            clock.advance(0.01); // Very small time advances
        }

        let initial_count = limiter.client_state.len();

        // Cleanup with a very short threshold - should preserve all recent clients
        limiter.cleanup_stale_clients(1_000_000); // 1ms

        assert_eq!(limiter.client_state.len(), initial_count);
    }
}
