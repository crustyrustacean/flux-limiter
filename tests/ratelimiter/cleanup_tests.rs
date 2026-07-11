// tests/ratelimiter/cleanup_tests.rs

#[cfg(test)]
mod tests {
    use crate::helpers::{assert_request_allowed, setup_limiter};

    #[test]
    fn cleanup_removes_stale_clients() {
        let (limiter, clock) = setup_limiter(1.0, 0.0, 0.0);

        // Add some clients at different times
        assert_request_allowed(&limiter, "client1"); // TAT = t=1

        clock.set_time(5.0);
        assert_request_allowed(&limiter, "client2"); // TAT = t=6

        clock.set_time(10.0);
        assert_request_allowed(&limiter, "client3"); // TAT = t=11

        // Verify all clients are in the map
        assert_eq!(limiter.client_count(), 3);

        // Clean up clients older than 4.5 seconds at t=12
        // Cutoff will be 12 - 4.5 = 7.5, so keep TATs > 7.5
        clock.set_time(12.0);
        let threshold_nanos = (4.5 * 1_000_000_000.0) as u64;
        limiter
            .cleanup_stale_clients(threshold_nanos)
            .expect("Error with the system clock.");

        // Only client3 (TAT=11) should remain
        assert_eq!(limiter.client_count(), 1);
        assert!(!limiter.contains_client(&"client1".to_string()));
        assert!(!limiter.contains_client(&"client2".to_string()));
        assert!(limiter.contains_client(&"client3".to_string()));

        // Clean up all remaining clients
        limiter
            .cleanup_stale_clients(0)
            .expect("Error with the system clock.");
        assert_eq!(limiter.client_count(), 0);
    }

    #[test]
    fn cleanup_handles_empty_state() {
        let (limiter, _clock) = setup_limiter(1.0, 0.0, 0.0);

        // Cleanup on empty state should not panic
        limiter
            .cleanup_stale_clients(1000)
            .expect("Error with the system clock.");
        assert_eq!(limiter.client_count(), 0);
    }

    #[test]
    fn cleanup_preserves_recent_clients() {
        let (limiter, clock) = setup_limiter(10.0, 0.0, 100.0);

        // Add several recent clients
        for i in 0..5 {
            let client = format!("client{}", i);
            assert_request_allowed(&limiter, &client);
            clock.advance(0.01); // Very small time advances
        }

        let initial_count = limiter.client_count();

        // Cleanup with a very short threshold - should preserve all recent clients
        limiter
            .cleanup_stale_clients(1_000_000)
            .expect("Error with the system clock."); // 1ms

        assert_eq!(limiter.client_count(), initial_count);
    }
}
