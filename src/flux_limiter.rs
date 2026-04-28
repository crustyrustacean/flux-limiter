// src/flux_limiter.rs

// flux-limiter: A rate limiter based on the Generic Cell Rate Algorithm (GCRA).

// dependencies
use crate::clock::{Clock, SystemClock};
use crate::config::FluxLimiterConfig;
use crate::errors::FluxLimiterError;
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use std::hash::Hash;
use std::sync::Arc;

impl<T, C> Clone for FluxLimiter<T, C>
where
    T: Hash + Eq + Clone,
    C: Clock + Clone,
{
    fn clone(&self) -> Self {
        Self {
            rate_nanos: self.rate_nanos,
            tolerance_nanos: self.tolerance_nanos,
            client_state: Arc::clone(&self.client_state),
            clock: self.clock.clone(),
        }
    }
}

/// The main FluxLimiter model.
/// T is the type used to identify clients (e.g., String, u64, etc.).
/// C is the clock type, defaulting to SystemClock.
/// We use `Arc<DashMap>` for thread-safe concurrent access to client state.
#[derive(Debug)]
pub struct FluxLimiter<T, C = SystemClock>
where
    T: Hash + Eq + Clone,
    C: Clock,
{
    rate_nanos: u64,
    tolerance_nanos: u64,
    pub(crate) client_state: Arc<DashMap<T, u64>>,
    clock: C,
}

// methods for the RateLimiter type
impl<T, C> FluxLimiter<T, C>
where
    T: Hash + Eq + Clone,
    C: Clock,
{
    // method to create a new flux limiter given a desired rate and burst value
    fn new(rate_per_second: f64, burst_capacity: f64, clock: C) -> Result<Self, FluxLimiterError> {
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

    // method to create a new flux limiter from a config object
    pub fn with_config(config: FluxLimiterConfig, clock: C) -> Result<Self, FluxLimiterError> {
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

    /// Returns the number of active clients currently tracked.
    pub fn client_count(&self) -> usize {
        self.client_state.len()
    }

    /// Returns `true` if the given client ID is currently tracked.
    pub fn contains_client(&self, client_id: &T) -> bool {
        self.client_state.contains_key(client_id)
    }

    pub fn check_request(&self, client_id: T) -> Result<FluxLimiterDecision, FluxLimiterError> {
        let current_time_nanos = self.clock.now()?;

        // Use DashMap entry API for atomic read-modify-write to prevent
        // TOCTOU race conditions when the same client makes concurrent requests.
        match self.client_state.entry(client_id) {
            Entry::Occupied(mut occupied) => {
                let previous_tat_nanos = *occupied.get();
                let is_conforming =
                    current_time_nanos >= previous_tat_nanos.saturating_sub(self.tolerance_nanos);

                if is_conforming {
                    let new_tat_nanos = current_time_nanos.max(previous_tat_nanos) + self.rate_nanos;
                    occupied.insert(new_tat_nanos);

                    Ok(FluxLimiterDecision {
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

                    Ok(FluxLimiterDecision {
                        allowed: false,
                        retry_after_seconds: Some(retry_after_nanos as f64 / 1_000_000_000.0),
                        remaining_capacity: Some(0.0),
                        reset_time_nanos: previous_tat_nanos,
                    })
                }
            }
            Entry::Vacant(vacant) => {
                // First request from this client is always allowed
                let new_tat_nanos = current_time_nanos + self.rate_nanos;
                vacant.insert(new_tat_nanos);

                Ok(FluxLimiterDecision {
                    allowed: true,
                    retry_after_seconds: None,
                    remaining_capacity: Some(
                        self.calculate_remaining_capacity(current_time_nanos, new_tat_nanos),
                    ),
                    reset_time_nanos: new_tat_nanos,
                })
            }
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
    pub fn cleanup_stale_clients(&self, max_stale_nanos: u64) -> Result<(), FluxLimiterError> {
        let current_time_nanos = self.clock.now()?;
        self.client_state.retain(|_, &mut tat| {
            tat + self.tolerance_nanos > current_time_nanos.saturating_sub(max_stale_nanos)
        });

        Ok(())
    }
}

/// Result of a rate limiting decision with metadata for HTTP responses
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct FluxLimiterDecision {
    /// Whether the request should be allowed
    pub allowed: bool,
    /// Seconds until the client can make another request (when denied)
    pub retry_after_seconds: Option<f64>,
    /// Approximate remaining burst capacity
    pub remaining_capacity: Option<f64>,
    /// When the rate limit window resets (nanoseconds since epoch)
    pub reset_time_nanos: u64,
}
