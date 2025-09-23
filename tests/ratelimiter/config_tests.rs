// tests/ratelimiter/config_tests.rs

#[cfg(test)]
mod tests {
    use crate::fixtures::test_clock::TestClock;
    use flux_limiter::{FluxLimiter, FluxLimiterConfig, FluxLimiterError};

    // Config validation tests
    #[test]
    fn config_rejects_zero_rate() {
        let config = FluxLimiterConfig::new(0.0, 1.0);
        let result = config.validate();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), FluxLimiterError::InvalidRate));
    }

    #[test]
    fn config_rejects_negative_rate() {
        let config = FluxLimiterConfig::new(-1.0, 1.0);
        let result = config.validate();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), FluxLimiterError::InvalidRate));
    }

    #[test]
    fn config_rejects_negative_burst() {
        let config = FluxLimiterConfig::new(1.0, -1.0);
        let result = config.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            FluxLimiterError::InvalidBurst
        ));
    }

    #[test]
    fn config_accepts_valid_parameters() {
        let config = FluxLimiterConfig::new(10.0, 5.0);
        let result = config.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn config_accepts_zero_burst() {
        let config = FluxLimiterConfig::new(1.0, 0.0);
        let result = config.validate();
        assert!(result.is_ok());
    }

    // Test config builder pattern
    #[test]
    fn config_builder_pattern_works() {
        let config = FluxLimiterConfig::new(0.0, 0.0).rate(10.0).burst(5.0);

        assert!(config.validate().is_ok());

        let clock = TestClock::new(0.0);
        let limiter = FluxLimiter::<String, _>::with_config(config, clock).unwrap();
        assert_eq!(limiter.rate(), 10.0);
        assert_eq!(limiter.burst(), 5.0);
    }

    // Constructor tests with config
    #[test]
    fn constructor_with_invalid_config_fails() {
        let clock = TestClock::new(0.0);
        let config = FluxLimiterConfig::new(0.0, 1.0);
        let result = FluxLimiter::<String, _>::with_config(config, clock);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), FluxLimiterError::InvalidRate));
    }

    #[test]
    fn constructor_with_valid_config_succeeds() {
        let clock = TestClock::new(0.0);
        let config = FluxLimiterConfig::new(10.0, 5.0);
        let result = FluxLimiter::<String, _>::with_config(config, clock);
        assert!(result.is_ok());
    }
}
