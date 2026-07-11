// tests/ratelimiter/helpers/mod.rs

// Import and re-export commonly used types for convenience in tests
pub use crate::fixtures::test_clock::TestClock;
pub use flux_limiter::{FluxLimiter, FluxLimiterConfig, FluxLimiterDecision, FluxLimiterError};

/// Creates a standard test setup with FluxLimiter, TestClock, and config.
///
/// # Arguments
/// * `rate` - Requests per second
/// * `burst` - Burst capacity
/// * `initial_time` - Initial clock time in seconds
///
/// # Returns
/// A tuple of (FluxLimiter, TestClock) ready for testing
pub fn setup_limiter(
    rate: f64,
    burst: f64,
    initial_time: f64,
) -> (FluxLimiter<String, TestClock>, TestClock) {
    let clock = TestClock::new(initial_time);
    let config = FluxLimiterConfig::new(rate, burst);
    let limiter = FluxLimiter::with_config(config, clock.clone())
        .expect("Valid configuration should create limiter");
    (limiter, clock)
}

/// Creates a FluxLimiter with standard test settings (10 req/sec, 5 burst).
///
/// # Returns
/// A tuple of (FluxLimiter, TestClock) with common test configuration
pub fn setup_standard_limiter() -> (FluxLimiter<String, TestClock>, TestClock) {
    setup_limiter(10.0, 5.0, 0.0)
}

/// Makes a request and asserts it was allowed.
///
/// # Arguments
/// * `limiter` - The FluxLimiter instance
/// * `client_id` - Client identifier
///
/// # Returns
/// The FluxLimiterDecision for further assertions if needed
pub fn assert_request_allowed<T: AsRef<str>>(
    limiter: &FluxLimiter<String, TestClock>,
    client_id: T,
) -> FluxLimiterDecision {
    let decision = limiter
        .check_request(client_id.as_ref().to_string())
        .expect("Request should succeed");
    assert!(decision.allowed, "Request should be allowed");
    decision
}

/// Makes a request and asserts it was denied.
///
/// # Arguments
/// * `limiter` - The FluxLimiter instance
/// * `client_id` - Client identifier
///
/// # Returns
/// The FluxLimiterDecision for further assertions if needed
pub fn assert_request_denied<T: AsRef<str>>(
    limiter: &FluxLimiter<String, TestClock>,
    client_id: T,
) -> FluxLimiterDecision {
    let decision = limiter
        .check_request(client_id.as_ref().to_string())
        .expect("Request should succeed");
    assert!(!decision.allowed, "Request should be denied");
    decision
}

/// Makes a request and asserts it resulted in a clock error.
///
/// # Arguments
/// * `limiter` - The FluxLimiter instance
/// * `client_id` - Client identifier
pub fn assert_request_clock_error<T: AsRef<str>>(
    limiter: &FluxLimiter<String, TestClock>,
    client_id: T,
) {
    let result = limiter.check_request(client_id.as_ref().to_string());
    assert!(result.is_err(), "Request should fail");
    match result.unwrap_err() {
        FluxLimiterError::ClockError(_) => {} // Expected
        other => panic!("Expected ClockError, got: {:?}", other),
    }
}

/// Makes multiple requests in sequence and returns all decisions.
///
/// # Arguments
/// * `limiter` - The FluxLimiter instance
/// * `client_id` - Client identifier
/// * `count` - Number of requests to make
///
/// # Returns
/// Vector of FluxLimiterDecision results
pub fn make_requests<T: AsRef<str>>(
    limiter: &FluxLimiter<String, TestClock>,
    client_id: T,
    count: usize,
) -> Vec<FluxLimiterDecision> {
    (0..count)
        .map(|_| {
            limiter
                .check_request(client_id.as_ref().to_string())
                .expect("Request should succeed")
        })
        .collect()
}

/// Tests a sequence of requests with expected outcomes.
///
/// # Arguments
/// * `limiter` - The FluxLimiter instance
/// * `client_id` - Client identifier
/// * `expected_outcomes` - Vector of expected allowed/denied outcomes
pub fn assert_request_sequence<T: AsRef<str>>(
    limiter: &FluxLimiter<String, TestClock>,
    client_id: T,
    expected_outcomes: &[bool],
) {
    for (i, &expected_allowed) in expected_outcomes.iter().enumerate() {
        let decision = limiter
            .check_request(client_id.as_ref().to_string())
            .expect("Request should succeed");
        assert_eq!(
            decision.allowed,
            expected_allowed,
            "Request {} expected allowed={}, got allowed={}",
            i + 1,
            expected_allowed,
            decision.allowed
        );
    }
}

/// Asserts that retry_after_seconds is within an expected range.
///
/// # Arguments
/// * `decision` - The FluxLimiterDecision to check
/// * `min_seconds` - Minimum expected retry time
/// * `max_seconds` - Maximum expected retry time
pub fn assert_retry_after_in_range(
    decision: &FluxLimiterDecision,
    min_seconds: f64,
    max_seconds: f64,
) {
    match decision.retry_after_seconds {
        Some(retry_after) => {
            assert!(
                retry_after >= min_seconds && retry_after <= max_seconds,
                "Expected retry_after between {} and {} seconds, got {}",
                min_seconds,
                max_seconds,
                retry_after
            );
        }
        None => panic!("Expected retry_after_seconds to be Some, got None"),
    }
}

/// Asserts that remaining capacity is within an expected range.
///
/// # Arguments
/// * `decision` - The FluxLimiterDecision to check
/// * `min_capacity` - Minimum expected remaining capacity
/// * `max_capacity` - Maximum expected remaining capacity
pub fn assert_remaining_capacity_in_range(
    decision: &FluxLimiterDecision,
    min_capacity: f64,
    max_capacity: f64,
) {
    match decision.remaining_capacity {
        Some(remaining) => {
            assert!(
                remaining >= min_capacity && remaining <= max_capacity,
                "Expected remaining capacity between {} and {}, got {}",
                min_capacity,
                max_capacity,
                remaining
            );
        }
        None => panic!("Expected remaining_capacity to be Some, got None"),
    }
}

/// Verifies that a configuration error matches the expected type.
///
/// # Arguments
/// * `result` - Result from FluxLimiter::with_config
/// * `expected_error` - Expected error type
pub fn assert_config_error(
    result: Result<FluxLimiter<String, TestClock>, FluxLimiterError>,
    expected_error: FluxLimiterError,
) {
    assert!(result.is_err(), "Expected configuration error");
    let actual_error = result.unwrap_err();
    match (actual_error, expected_error) {
        (FluxLimiterError::InvalidRate, FluxLimiterError::InvalidRate) => {}
        (FluxLimiterError::InvalidBurst, FluxLimiterError::InvalidBurst) => {}
        (actual, expected) => {
            panic!("Expected error {:?}, got {:?}", expected, actual);
        }
    }
}

/// Creates multiple clients with the given prefix and makes requests for each.
///
/// # Arguments
/// * `limiter` - The FluxLimiter instance
/// * `client_prefix` - Prefix for client IDs (will be appended with numbers)
/// * `count` - Number of clients to create
///
/// # Returns
/// Vector of (client_id, decision) tuples
pub fn make_requests_for_multiple_clients(
    limiter: &FluxLimiter<String, TestClock>,
    client_prefix: &str,
    count: usize,
) -> Vec<(String, FluxLimiterDecision)> {
    (0..count)
        .map(|i| {
            let client_id = format!("{}{}", client_prefix, i);
            let decision = limiter
                .check_request(client_id.clone())
                .expect("Request should succeed");
            (client_id, decision)
        })
        .collect()
}

/// Advances clock time and makes a request, returning the decision.
///
/// # Arguments
/// * `limiter` - The FluxLimiter instance
/// * `clock` - The TestClock instance
/// * `time_advance` - Seconds to advance the clock
/// * `client_id` - Client identifier
///
/// # Returns
/// The FluxLimiterDecision after time advancement
pub fn advance_time_and_request<T: AsRef<str>>(
    limiter: &FluxLimiter<String, TestClock>,
    clock: &TestClock,
    time_advance: f64,
    client_id: T,
) -> FluxLimiterDecision {
    clock.advance(time_advance);
    limiter
        .check_request(client_id.as_ref().to_string())
        .expect("Request should succeed")
}
