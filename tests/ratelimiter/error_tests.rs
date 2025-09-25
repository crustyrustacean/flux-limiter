// tests/ratelimiter/error_tests.rs

#[cfg(test)]
mod tests {
    use crate::fixtures::test_clock::TestClock;
    use flux_limiter::{FluxLimiter, FluxLimiterConfig, FluxLimiterError};

    #[test]
    fn clock_error_propagates_in_check_request() {
        let clock = TestClock::new(0.0);
        let config = FluxLimiterConfig::new(10.0, 5.0);
        let limiter = FluxLimiter::with_config(config, clock.clone()).unwrap();

        // Make the clock fail on next call
        clock.fail_next_call();

        let result = limiter.check_request("client1");
        assert!(result.is_err());

        // Verify it's specifically a clock error
        match result.unwrap_err() {
            FluxLimiterError::ClockError(_) => {} // Expected
            other => panic!("Expected ClockError, got: {:?}", other),
        }
    }

    #[test]
    fn clock_recovery_after_failure() {
        let clock = TestClock::new(0.0);
        let config = FluxLimiterConfig::new(10.0, 5.0);
        let limiter = FluxLimiter::with_config(config, clock.clone()).unwrap();

        // First request should succeed
        let result1 = limiter.check_request("client1");
        assert!(result1.is_ok());
        assert!(result1.unwrap().allowed);

        // Make clock fail for next call
        clock.fail_next_call();
        let result2 = limiter.check_request("client1");
        assert!(result2.is_err());

        // Clock should work again automatically
        let result3 = limiter.check_request("client1");
        assert!(result3.is_ok());
    }

    #[test]
    fn clock_error_propagates_in_cleanup() {
        let clock = TestClock::new(0.0);
        let config = FluxLimiterConfig::new(10.0, 5.0);
        let limiter = FluxLimiter::with_config(config, clock.clone()).unwrap();

        // Add a client first
        let _ = limiter.check_request("client1").unwrap();

        // Make clock fail
        clock.fail_next_call();

        let result = limiter.cleanup_stale_clients(1000);
        assert!(result.is_err());

        match result.unwrap_err() {
            FluxLimiterError::ClockError(_) => {} // Expected
            other => panic!("Expected ClockError, got: {:?}", other),
        }
    }

    #[test]
    fn multiple_clock_failures() {
        let clock = TestClock::new(0.0);
        let config = FluxLimiterConfig::new(5.0, 2.0);
        let limiter = FluxLimiter::with_config(config, clock.clone()).unwrap();

        // Test multiple consecutive failures
        for i in 0..3 {
            clock.fail_next_call();
            let result = limiter.check_request(format!("client{}", i));
            assert!(result.is_err(), "Attempt {} should have failed", i);
        }

        // Should work again after failures
        let result = limiter.check_request("client_recovery".to_string());
        assert!(result.is_ok());
        assert!(result.unwrap().allowed);
    }

    #[test]
    fn config_validation_errors_still_work() {
        let clock = TestClock::new(0.0);

        // Test invalid rate
        let config = FluxLimiterConfig::new(0.0, 5.0);
        let result = FluxLimiter::<String, _>::with_config(config, clock.clone());
        assert!(result.is_err());
        match result.unwrap_err() {
            FluxLimiterError::InvalidRate => {} // Expected
            other => panic!("Expected InvalidRate, got: {:?}", other),
        }

        // Test invalid burst
        let config = FluxLimiterConfig::new(10.0, -1.0);
        let result = FluxLimiter::<String, _>::with_config(config, clock.clone());
        assert!(result.is_err());
        match result.unwrap_err() {
            FluxLimiterError::InvalidBurst => {} // Expected
            other => panic!("Expected InvalidBurst, got: {:?}", other),
        }
    }

    #[test]
    fn partial_operations_with_clock_failure() {
        let clock = TestClock::new(0.0);
        let config = FluxLimiterConfig::new(2.0, 1.0);
        let limiter = FluxLimiter::with_config(config, clock.clone()).unwrap();

        // Successfully add some clients
        assert!(limiter.check_request("client1").unwrap().allowed);
        clock.advance(0.1);
        assert!(limiter.check_request("client2").unwrap().allowed);

        // Verify clients are in the map
        assert_eq!(limiter.client_state.len(), 2);

        // Clock fails during operation
        clock.fail_next_call();
        let result = limiter.check_request("client3");
        assert!(result.is_err());

        // Previous clients should still be in the map (operation didn't complete)
        assert_eq!(limiter.client_state.len(), 2);

        // Should work again after clock recovery
        let result = limiter.check_request("client3");
        assert!(result.is_ok());
        assert_eq!(limiter.client_state.len(), 3);
    }

    #[test]
    fn error_display_formatting() {
        let clock = TestClock::new(0.0);
        let config = FluxLimiterConfig::new(10.0, 5.0);
        let limiter = FluxLimiter::with_config(config, clock.clone()).unwrap();

        clock.fail_next_call();
        let result = limiter.check_request("client1");

        match result {
            Err(e) => {
                let error_string = format!("{}", e);
                assert!(!error_string.is_empty());
                // Should contain some indication it's a clock error
                assert!(
                    error_string.to_lowercase().contains("clock")
                        || error_string.to_lowercase().contains("time")
                );
            }
            Ok(_) => panic!("Expected error, got success"),
        }
    }

    #[test]
    fn cleanup_recovers_from_clock_error() {
        let clock = TestClock::new(0.0);
        let config = FluxLimiterConfig::new(10.0, 5.0);
        let limiter = FluxLimiter::with_config(config, clock.clone()).unwrap();

        // Add some clients
        let _ = limiter.check_request("client1").unwrap();
        let _ = limiter.check_request("client2").unwrap();
        assert_eq!(limiter.client_state.len(), 2);

        // Cleanup fails due to clock error
        clock.fail_next_call();
        let result = limiter.cleanup_stale_clients(1000);
        assert!(result.is_err());

        // Clients should still be there (cleanup didn't succeed)
        assert_eq!(limiter.client_state.len(), 2);

        // Cleanup should work after clock recovery
        clock.advance(2.0); // Move time forward
        let result = limiter.cleanup_stale_clients(1_000_000_000); // 1 second threshold
        assert!(result.is_ok());
    }
}
