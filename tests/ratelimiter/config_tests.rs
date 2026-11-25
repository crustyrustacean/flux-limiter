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

    // Non-finite and out-of-range values (issue #4)
    #[test]
    fn config_rejects_nan_rate() {
        let config = FluxLimiterConfig::new(f64::NAN, 1.0);
        assert!(matches!(
            config.validate().unwrap_err(),
            FluxLimiterError::InvalidRate
        ));
    }

    #[test]
    fn config_rejects_infinite_rate() {
        for rate in [f64::INFINITY, f64::NEG_INFINITY] {
            let config = FluxLimiterConfig::new(rate, 1.0);
            assert!(
                matches!(
                    config.validate().unwrap_err(),
                    FluxLimiterError::InvalidRate
                ),
                "rate {rate} should be rejected"
            );
        }
    }

    #[test]
    fn config_rejects_nan_burst() {
        let config = FluxLimiterConfig::new(1.0, f64::NAN);
        assert!(matches!(
            config.validate().unwrap_err(),
            FluxLimiterError::InvalidBurst
        ));
    }

    #[test]
    fn config_rejects_infinite_burst() {
        for burst in [f64::INFINITY, f64::NEG_INFINITY] {
            let config = FluxLimiterConfig::new(1.0, burst);
            assert!(
                matches!(
                    config.validate().unwrap_err(),
                    FluxLimiterError::InvalidBurst
                ),
                "burst {burst} should be rejected"
            );
        }
    }

    #[test]
    fn config_rejects_rate_above_one_billion() {
        // A finite rate above 1e9 truncates to a zero interval.
        for rate in [2e9, 1e12] {
            let config = FluxLimiterConfig::new(rate, 1.0);
            assert!(
                matches!(
                    config.validate().unwrap_err(),
                    FluxLimiterError::InvalidRate
                ),
                "rate {rate} should be rejected"
            );
        }
    }

    #[test]
    fn config_rejects_smallest_rate_above_the_bound() {
        // The very next f64 above 1e9 already truncates the interval to zero,
        // so the bound is inclusive of 1e9 and exclusive of anything larger.
        let just_above = f64::from_bits(1e9f64.to_bits() + 1);
        assert!(just_above > 1e9);
        let config = FluxLimiterConfig::new(just_above, 1.0);
        assert!(matches!(
            config.validate().unwrap_err(),
            FluxLimiterError::InvalidRate
        ));
    }

    #[test]
    fn config_accepts_rate_of_exactly_one_billion() {
        let config = FluxLimiterConfig::new(1e9, 1.0);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn config_reports_rate_error_before_burst_error() {
        let config = FluxLimiterConfig::new(f64::NAN, f64::NAN);
        assert!(matches!(
            config.validate().unwrap_err(),
            FluxLimiterError::InvalidRate
        ));
    }

    #[test]
    fn validate_and_constructor_agree_on_rejected_configs() {
        let cases: [(f64, f64, FluxLimiterError); 8] = [
            (f64::NAN, 1.0, FluxLimiterError::InvalidRate),
            (f64::INFINITY, 1.0, FluxLimiterError::InvalidRate),
            (f64::NEG_INFINITY, 1.0, FluxLimiterError::InvalidRate),
            (2e9, 1.0, FluxLimiterError::InvalidRate),
            (1.0, f64::NAN, FluxLimiterError::InvalidBurst),
            (1.0, f64::INFINITY, FluxLimiterError::InvalidBurst),
            (1.0, f64::NEG_INFINITY, FluxLimiterError::InvalidBurst),
            (1.0, -1.0, FluxLimiterError::InvalidBurst),
        ];

        for (rate, burst, expected) in cases {
            let config = FluxLimiterConfig::new(rate, burst);
            let validated = config.validate().unwrap_err();
            assert!(
                same_variant(&validated, &expected),
                "validate() gave {validated:?}, expected {expected:?}"
            );

            let clock = TestClock::new(0.0);
            let constructed =
                FluxLimiter::<String, _>::with_config(FluxLimiterConfig::new(rate, burst), clock);
            let constructed = constructed.unwrap_err();
            assert!(
                same_variant(&constructed, &expected),
                "with_config() gave {constructed:?}, expected {expected:?}"
            );
        }
    }

    /// `FluxLimiterError` does not implement `PartialEq`, so compare by variant.
    fn same_variant(actual: &FluxLimiterError, expected: &FluxLimiterError) -> bool {
        matches!(
            (actual, expected),
            (FluxLimiterError::InvalidRate, FluxLimiterError::InvalidRate)
                | (
                    FluxLimiterError::InvalidBurst,
                    FluxLimiterError::InvalidBurst
                )
                | (
                    FluxLimiterError::ClockError(_),
                    FluxLimiterError::ClockError(_)
                )
        )
    }

    // Construction: a finite burst whose tolerance would not fit in u64 (issue #4)
    #[test]
    fn constructor_rejects_finite_burst_too_large_to_fit() {
        // rate = 1 -> T = 1e9 ns. burst * T must stay below 2^64, so anything
        // above ~1.8e10 would saturate the cast to u64::MAX (an unlimited burst).
        for burst in [1.8446745e10, 1e11, f64::MAX] {
            let config = FluxLimiterConfig::new(1.0, burst);
            // It is finite and non-negative, so validate() alone accepts it...
            assert!(config.validate().is_ok(), "burst {burst}");
            // ...and construction is what rejects it.
            let result = FluxLimiter::<String, _>::with_config(config, TestClock::new(0.0));
            assert!(
                matches!(result.unwrap_err(), FluxLimiterError::InvalidBurst),
                "burst {burst} should be rejected at construction"
            );
        }
    }

    #[test]
    fn constructor_accepts_burst_just_below_the_fit_limit() {
        // The largest f64 strictly below 2^64 is representable as a u64 tolerance.
        let largest_fitting = f64::from_bits((u64::MAX as f64).to_bits() - 1);
        let burst = largest_fitting / 1e9; // at rate = 1, T = 1e9 ns
        let config = FluxLimiterConfig::new(1.0, burst);
        assert!(FluxLimiter::<String, _>::with_config(config, TestClock::new(0.0)).is_ok());
    }

    #[test]
    fn constructor_rejects_tiny_rate() {
        // rate = 1e-12 -> interval 1e21 ns, which saturates to u64::MAX and
        // previously overflowed on a client's first request.
        for rate in [1e-12, 1e-100] {
            let result = FluxLimiter::<String, _>::with_config(
                FluxLimiterConfig::new(rate, 1.0),
                TestClock::new(0.0),
            );
            assert!(
                matches!(result.unwrap_err(), FluxLimiterError::InvalidRate),
                "rate {rate} should be rejected"
            );
        }
    }

    #[test]
    fn rate_accessor_is_exact_for_whole_nanosecond_intervals() {
        for rate in [1e9, 1e8, 1e6, 1000.0, 10.0, 1.0] {
            let limiter = FluxLimiter::<u64, _>::with_config(
                FluxLimiterConfig::new(rate, 1.0),
                TestClock::new(0.0),
            )
            .unwrap();
            assert_eq!(limiter.rate(), rate, "rate {rate}");
        }
    }

    #[test]
    fn burst_accessor_is_exact_for_whole_nanosecond_tolerances() {
        for (rate, burst) in [(10.0, 5.0), (1.0, 3.0), (1000.0, 0.5), (1e8, 7.0)] {
            let limiter = FluxLimiter::<u64, _>::with_config(
                FluxLimiterConfig::new(rate, burst),
                TestClock::new(0.0),
            )
            .unwrap();
            assert_eq!(limiter.burst(), burst, "rate {rate} burst {burst}");
        }
    }
}
