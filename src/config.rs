// src/config.rs

//! Configuration types for the flux limiter

// dependencies
use crate::errors::RateLimiterError;

/// Configuration for rate limiter behavior.
///
/// # Examples
///
/// ```rust
/// use flux_limiter::RateLimiterConfig;
///
/// let config = RateLimiterConfig::new(10.0, 5.0)
///     .rate(20.0)
///     .burst(10.0);
/// ```

#[derive(Debug, Clone)]
pub struct RateLimiterConfig {
    pub(crate) rate_per_second: f64,
    pub(crate) burst_capacity: f64,
}

impl RateLimiterConfig {
    /// Create a new configuration with rate and burst settings.
    ///
    /// # Arguments
    /// 
    /// * `rate_per_second` - Maximum sustained rate (must be > 0)
    /// * `burst_capacity` - Additional burst allowance (must be ≥ 0)
    pub fn new(rate_per_second: f64, burst_capacity: f64) -> Self {
        Self {
            rate_per_second,
            burst_capacity,
        }
    }

    /// Builder-style: set rate per second
    pub fn rate(mut self, rate_per_second: f64) -> Self {
        self.rate_per_second = rate_per_second;
        self
    }

    /// Builder-style: set burst capacity  
    pub fn burst(mut self, burst_capacity: f64) -> Self {
        self.burst_capacity = burst_capacity;
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), RateLimiterError> {
        if self.rate_per_second <= 0.0 {
            return Err(RateLimiterError::InvalidRate);
        }
        if self.burst_capacity < 0.0 {
            return Err(RateLimiterError::InvalidBurst);
        }
        Ok(())
    }
}
