// src/config.rs

//! Configuration types for the flux limiter

// dependencies
use crate::errors::FluxLimiterError;

/// The largest rate `FluxLimiterConfig` accepts, in requests per second.
///
/// The rate interval is stored as whole nanoseconds (`1e9 / rate`), so any rate
/// above this would truncate to a zero interval and never limit anything.
pub(crate) const MAX_RATE_PER_SECOND: f64 = 1_000_000_000.0;

/// Configuration for rate limiter behavior.
///
/// # Examples
///
/// ```rust
/// use flux_limiter::FluxLimiterConfig;
///
/// let config = FluxLimiterConfig::new(10.0, 5.0)
///     .rate(20.0)
///     .burst(10.0);
/// ```

#[derive(Debug, Clone)]
pub struct FluxLimiterConfig {
    pub(crate) rate_per_second: f64,
    pub(crate) burst_capacity: f64,
}

impl FluxLimiterConfig {
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
    pub fn validate(&self) -> Result<(), FluxLimiterError> {
        // `NaN` fails every comparison, so neither `<= 0.0` nor `> MAX_RATE` alone
        // would catch it; `is_finite` rejects NaN and both infinities explicitly.
        if !self.rate_per_second.is_finite()
            || self.rate_per_second <= 0.0
            || self.rate_per_second > MAX_RATE_PER_SECOND
        {
            return Err(FluxLimiterError::InvalidRate);
        }
        if !self.burst_capacity.is_finite() || self.burst_capacity < 0.0 {
            return Err(FluxLimiterError::InvalidBurst);
        }
        Ok(())
    }
}
