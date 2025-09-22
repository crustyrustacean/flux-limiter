// lib/rate_limiter.rs

// flux-limiter: A rate limiter based on the Generic Cell Rate Algorithm (GCRA).

// dependencies
use crate::clock::{Clock, SystemClock};
use crate::config::RateLimiterConfig;
use crate::errors::RateLimiterError;
use dashmap::DashMap;
use std::hash::Hash;
use std::sync::Arc;

/// The main RateLimiter model.
/// T is the type used to identify clients (e.g., String, u64, etc.).
/// C is the clock type, defaulting to SystemClock.
/// We use `Arc<DashMap>` for thread-safe concurrent access to client state.
#[derive(Debug)]
pub struct RateLimiter<T, C = SystemClock>
where
    T: Hash + Eq + Clone,
    C: Clock,
{
    rate_nanos: u64,
    tolerance_nanos: u64,
    client_state: Arc<DashMap<T, u64>>,
    clock: C,
}

// methods for the RateLimiter type
impl<T, C> RateLimiter<T, C>
where
    T: Hash + Eq + Clone,
    C: Clock,
{
    // method to create a new rate limiter given a desired rate and burst value
    fn new(rate_per_second: f64, burst_capacity: f64, clock: C) -> Result<Self, RateLimiterError> {
        // Convert to nanoseconds
        let rate_nanos = (1_000_000_000.0 / rate_per_second) as u64;
        let tolerance_nanos = (burst_capacity * rate_nanos as f64) as u64;

        Ok(Self {
            rate_nanos,
            tolerance_nanos,
            client_state: Arc::new(DashMap::new()),
            clock,
        })
    }

    // method to create a new rate limiter from a config object
    pub fn with_config(config: RateLimiterConfig, clock: C) -> Result<Self, RateLimiterError> {
        config.validate()?;
        Self::new(config.rate_per_second, config.burst_capacity, clock)
    }

    // accessor method to return the rate field (convert back to requests per second)
    pub fn rate(&self) -> f64 {
        1_000_000_000.0 / self.rate_nanos as f64
    }

    // accessor method to return the burst field (convert back to burst capacity)
    pub fn burst(&self) -> f64 {
        self.tolerance_nanos as f64 / self.rate_nanos as f64
    }

    // internal method to get the increment in nanoseconds
    #[allow(dead_code)]
    fn increment_nanos(&self) -> u64 {
        self.rate_nanos
    }

    // Optional: internal method to get the tolerance in nanoseconds
    #[allow(dead_code)]
    fn tolerance_nanos(&self) -> u64 {
        self.tolerance_nanos
    }

    // Optional: keep the old method names for backwards compatibility
    #[allow(dead_code)]
    fn increment(&self) -> f64 {
        self.rate_nanos as f64 / 1_000_000_000.0
    }

    // Optional: internal method to get the tolerance in seconds
    #[allow(dead_code)]
    fn tolerance(&self) -> f64 {
        self.tolerance_nanos as f64 / 1_000_000_000.0
    }

    pub fn check_request(&self, client_id: T) -> Result<RateLimitDecision, RateLimiterError> {
        let current_time_nanos = self.clock.now();
        let previous_tat_nanos = self
            .client_state
            .get(&client_id)
            .map(|entry| *entry.value())
            .unwrap_or(current_time_nanos);

        let is_conforming =
            current_time_nanos >= previous_tat_nanos.saturating_sub(self.tolerance_nanos);

        if is_conforming {
            let new_tat_nanos = current_time_nanos.max(previous_tat_nanos) + self.rate_nanos;
            self.client_state.insert(client_id, new_tat_nanos);

            Ok(RateLimitDecision {
                allowed: true,
                retry_after_seconds: None,
                remaining_capacity: Some(
                    self.calculate_remaining_capacity(current_time_nanos, new_tat_nanos),
                ),
                reset_time_nanos: new_tat_nanos,
            })
        } else {
            let retry_after_nanos = previous_tat_nanos
                .saturating_sub(self.tolerance_nanos)
                .saturating_sub(current_time_nanos);

            Ok(RateLimitDecision {
                allowed: false,
                retry_after_seconds: Some(retry_after_nanos as f64 / 1_000_000_000.0),
                remaining_capacity: Some(0.0),
                reset_time_nanos: previous_tat_nanos,
            })
        }
    }

    fn calculate_remaining_capacity(&self, current_time: u64, tat: u64) -> f64 {
        if current_time >= tat.saturating_sub(self.tolerance_nanos) {
            let time_until_tat = tat.saturating_sub(current_time) as f64 / 1_000_000_000.0;
            let rate_per_second = self.rate();
            (self.burst() - (time_until_tat * rate_per_second)).max(0.0)
        } else {
            0.0
        }
    }

    // method to clean up stale clients
    pub fn cleanup_stale_clients(&self, max_stale_nanos: u64) {
        let current_time_nanos = self.clock.now();
        self.client_state.retain(|_, &mut tat| {
            tat + self.tolerance_nanos > current_time_nanos.saturating_sub(max_stale_nanos)
        });
    }
}

/// Result of a rate limiting decision with metadata for HTTP responses
#[derive(Debug, Clone)]
pub struct RateLimitDecision {
    /// Whether the request should be allowed
    pub allowed: bool,
    /// Seconds until the client can make another request (when denied)
    pub retry_after_seconds: Option<f64>,
    /// Approximate remaining burst capacity
    pub remaining_capacity: Option<f64>,
    /// When the rate limit window resets (nanoseconds since epoch)
    pub reset_time_nanos: u64,
}

// Make SystemClock the default
impl Default for SystemClock {
    fn default() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    // Test clock implementation
    #[derive(Debug, Clone)]
    struct TestClock {
        time: Arc<AtomicU64>, // Store as nanos
    }

    impl TestClock {
        fn new(initial_time: f64) -> Self {
            Self {
                time: Arc::new(AtomicU64::new((initial_time * 1_000_000_000.0) as u64)),
            }
        }

        fn advance(&self, seconds: f64) {
            let nanos = (seconds * 1_000_000_000.0) as u64;
            self.time.fetch_add(nanos, Ordering::Relaxed);
        }

        fn set_time(&self, seconds: f64) {
            let nanos = (seconds * 1_000_000_000.0) as u64;
            self.time.store(nanos, Ordering::Relaxed);
        }

        // Helper to get time as f64 for test assertions
        fn time_as_f64(&self) -> f64 {
            self.time.load(Ordering::Relaxed) as f64 / 1_000_000_000.0
        }
    }

    impl Clock for TestClock {
        fn now(&self) -> u64 {
            self.time.load(Ordering::Relaxed)
        }
    }

    // Config validation tests
    #[test]
    fn config_rejects_zero_rate() {
        let config = RateLimiterConfig::new(0.0, 1.0);
        let result = config.validate();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RateLimiterError::InvalidRate));
    }

    #[test]
    fn config_rejects_negative_rate() {
        let config = RateLimiterConfig::new(-1.0, 1.0);
        let result = config.validate();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RateLimiterError::InvalidRate));
    }

    #[test]
    fn config_rejects_negative_burst() {
        let config = RateLimiterConfig::new(1.0, -1.0);
        let result = config.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RateLimiterError::InvalidBurst
        ));
    }

    #[test]
    fn config_accepts_valid_parameters() {
        let config = RateLimiterConfig::new(10.0, 5.0);
        let result = config.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn config_accepts_zero_burst() {
        let config = RateLimiterConfig::new(1.0, 0.0);
        let result = config.validate();
        assert!(result.is_ok());
    }

    // Constructor tests with config
    #[test]
    fn constructor_with_invalid_config_fails() {
        let clock = TestClock::new(0.0);
        let config = RateLimiterConfig::new(0.0, 1.0);
        let result = RateLimiter::<String, _>::with_config(config, clock);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RateLimiterError::InvalidRate));
    }

    #[test]
    fn constructor_with_valid_config_succeeds() {
        let clock = TestClock::new(0.0);
        let config = RateLimiterConfig::new(10.0, 5.0);
        let result = RateLimiter::<String, _>::with_config(config, clock);
        assert!(result.is_ok());
    }

    // GCRA algorithm tests
    #[test]
    fn first_request_always_allowed() {
        let clock = TestClock::new(0.0);
        let config = RateLimiterConfig::new(1.0, 1.0);
        let limiter = RateLimiter::with_config(config, clock).unwrap();
        let decision = limiter.check_request("client1").unwrap();
        assert!(decision.allowed);
    }

    #[test]
    fn rate_limiting_blocks_rapid_requests() {
        let clock = TestClock::new(0.0);
        let config = RateLimiterConfig::new(1.0, 0.0); // 1 req/sec, no burst
        let limiter = RateLimiter::with_config(config, clock.clone()).unwrap();
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
        let config = RateLimiterConfig::new(1.0, 3.0); // 1 req/sec, burst of 3
        let limiter = RateLimiter::with_config(config, clock.clone()).unwrap();
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
        let config = RateLimiterConfig::new(1.0, 0.0); // 1 req/sec, no burst
        let limiter = RateLimiter::with_config(config, clock.clone()).unwrap();

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
        let config = RateLimiterConfig::new(2.0, 0.0); // 2 req/sec, no burst
        let limiter = RateLimiter::with_config(config, clock.clone()).unwrap();
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
        let config = RateLimiterConfig::new(10.0, 5.0);
        let limiter = RateLimiter::<String, _>::with_config(config, clock).unwrap();

        // Test that accessors return the original user-provided values
        assert_eq!(limiter.rate(), 10.0);
        assert_eq!(limiter.burst(), 5.0);
    }

    #[test]
    fn nanosecond_precision() {
        let clock = TestClock::new(0.0);
        let config = RateLimiterConfig::new(1_000_000.0, 0.0); // 1M req/sec
        let limiter = RateLimiter::with_config(config, clock.clone()).unwrap();
        let client = "client1";

        // First request should be allowed
        assert!(limiter.check_request(client).unwrap().allowed);

        // Second request immediately should be blocked
        assert!(!limiter.check_request(client).unwrap().allowed);

        // Advance by exactly 1 microsecond (1000 nanoseconds)
        clock.advance(0.000001);
        assert!(limiter.check_request(client).unwrap().allowed);
    }

    // Test config builder pattern
    #[test]
    fn config_builder_pattern_works() {
        let config = RateLimiterConfig::new(0.0, 0.0).rate(10.0).burst(5.0);

        assert!(config.validate().is_ok());

        let clock = TestClock::new(0.0);
        let limiter = RateLimiter::<String, _>::with_config(config, clock).unwrap();
        assert_eq!(limiter.rate(), 10.0);
        assert_eq!(limiter.burst(), 5.0);
    }

    #[test]
    fn cleanup_removes_stale_clients() {
        let clock = TestClock::new(0.0);
        let config = RateLimiterConfig::new(1.0, 0.0);
        let limiter = RateLimiter::with_config(config, clock.clone()).unwrap();

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
        let config = RateLimiterConfig::new(1.0, 0.0);
        let limiter = RateLimiter::<String, _>::with_config(config, clock).unwrap();

        // Cleanup on empty state should not panic
        limiter.cleanup_stale_clients(1000);
        assert_eq!(limiter.client_state.len(), 0);
    }

    #[test]
    fn cleanup_preserves_recent_clients() {
        let clock = TestClock::new(100.0);
        let config = RateLimiterConfig::new(10.0, 0.0);
        let limiter = RateLimiter::with_config(config, clock.clone()).unwrap();

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

    // Tests for the new RateLimitDecision metadata
    #[test]
    fn check_request_returns_detailed_decision() {
        let clock = TestClock::new(0.0);
        let config = RateLimiterConfig::new(1.0, 2.0); // 1 req/sec, burst of 2
        let limiter = RateLimiter::with_config(config, clock.clone()).unwrap();
        let client = "client1";

        // First request should be allowed with metadata
        let decision = limiter.check_request(client).unwrap();
        assert!(decision.allowed);
        assert!(decision.retry_after_seconds.is_none());
        assert!(decision.remaining_capacity.is_some());
        assert!(decision.reset_time_nanos > 0);
    }

    #[test]
    fn retry_after_calculation_works() {
        let clock = TestClock::new(0.0);
        let config = RateLimiterConfig::new(2.0, 0.0); // 2 req/sec, no burst
        let limiter = RateLimiter::with_config(config, clock.clone()).unwrap();
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
        let config = RateLimiterConfig::new(1.0, 3.0); // 1 req/sec, burst of 3
        let limiter = RateLimiter::with_config(config, clock.clone()).unwrap();
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
