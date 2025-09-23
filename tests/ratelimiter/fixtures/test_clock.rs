// tests/ratelimiter/fixtures/test_clock.rs

// dependencies
use flux_limiter::clock::Clock;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

// Test clock implementation
#[derive(Debug, Clone)]
pub struct TestClock {
    time: Arc<AtomicU64>, // Store as nanos
}

impl TestClock {
    pub fn new(initial_time: f64) -> Self {
        Self {
            time: Arc::new(AtomicU64::new((initial_time * 1_000_000_000.0) as u64)),
        }
    }

    pub fn advance(&self, seconds: f64) {
        let nanos = (seconds * 1_000_000_000.0) as u64;
        self.time.fetch_add(nanos, Ordering::Relaxed);
    }

    pub fn set_time(&self, seconds: f64) {
        let nanos = (seconds * 1_000_000_000.0) as u64;
        self.time.store(nanos, Ordering::Relaxed);
    }

    // Helper to get time as f64 for test assertions
    pub fn time_as_f64(&self) -> f64 {
        self.time.load(Ordering::Relaxed) as f64 / 1_000_000_000.0
    }
}

impl Clock for TestClock {
    fn now(&self) -> u64 {
        self.time.load(Ordering::Relaxed)
    }
}
