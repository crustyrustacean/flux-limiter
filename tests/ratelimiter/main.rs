// tests/ratelimiter/main.rs

// dependencies
use flux_limiter::{RateLimiter, RateLimiterConfig, RateLimitDecision};

// test modules
mod fixtures;
mod helpers;
mod config_tests;
mod gcra_algorithm_tests;
mod decision_metadata_tests;
mod cleanup_tests;
mod performance_tests;

// Re-export common test utilities
pub use fixtures::test_clock::TestClock;
pub use helpers::assertions::*;