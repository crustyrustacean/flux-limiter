// src/errors.rs

// error handling for the flux limiter type

// dependencies
use std::error::Error;
use std::fmt;

use crate::clock::ClockError;

/// Error type for FluxLimiter configuration issues.
#[non_exhaustive]
#[derive(Debug)]
pub enum FluxLimiterError {
    InvalidRate,            // for rate <= 0
    InvalidBurst,           // for burst < 0
    ClockError(ClockError), // error variant for issues with the system clock
}

// implement the Display trait for the FluxLimiterError type
impl fmt::Display for FluxLimiterError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            FluxLimiterError::InvalidRate => write!(f, "Rate must be positive"),
            FluxLimiterError::InvalidBurst => write!(f, "Burst must be non-negative"),
            FluxLimiterError::ClockError(_) => {
                write!(f, "Clock error occurred")
            }
        }
    }
}

// implement the Error trait for the RateLimiter type
impl Error for FluxLimiterError {}
