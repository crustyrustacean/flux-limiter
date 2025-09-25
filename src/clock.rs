// src/clock.rs

// clock module definition and implementations

// dependencies
use std::time::{SystemTime, UNIX_EPOCH};

/// Clock trait to abstract time retrieval.
/// Implementors must be thread-safe (Send + Sync).
/// The `now` method returns the current time in nanoseconds as a u64.
/// This trait allows for different clock implementations, such as system time or a test clock.
/// The Clock trait is used by the RateLimiter to get the current time.
pub trait Clock: Send + Sync {
    fn now(&self) -> Result<u64, ClockError>;
}

/// Clock error type
#[derive(Debug)]
pub enum ClockError {
    SystemTimeError,
}

/// SystemClock implementation using the system time.
/// Returns the current time in nanoseconds since the Unix epoch.
/// Panics if the system clock is before the Unix epoch.
/// This is the default clock used in the RateLimiter.
/// Implements the Clock trait.
/// Thread-safe and can be shared across threads.
#[derive(Debug, Clone)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Result<u64, ClockError> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .map_err(|_| ClockError::SystemTimeError)
    }
}
