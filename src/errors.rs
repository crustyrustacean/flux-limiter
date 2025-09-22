// src/errors.rs

// error handling for the rate limiter type

// dependencies
use std::error::Error;
use std::fmt;

// enum type to represent errors related to the rate limiter type
#[non_exhaustive]
#[derive(Debug)]
pub enum RateLimiterError {
    InvalidRate,  // for rate <= 0
    InvalidBurst, // for burst < 0
}

// implement the Display trait for the RateLimiterError type
impl fmt::Display for RateLimiterError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RateLimiterError::InvalidRate => write!(f, "Rate must be positive"),
            RateLimiterError::InvalidBurst => write!(f, "Burst must be non-negative"),
        }
    }
}

// implement the Error trait for the RateLimiter type
impl Error for RateLimiterError {}
