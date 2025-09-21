// src/lib/clock.rs

// dependencies
#[cfg(any(test, feature = "testing"))]
use std::sync::Arc;
#[cfg(any(test, feature = "testing"))]
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

// Clock trait definition
pub trait Clock: Send + Sync {
    fn now(&self) -> u64;
}

// Default implementation using SystemTime
#[derive(Debug, Clone)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System clock went backwards before Unix epoch")
            .as_nanos() as u64
    }
}

// Test clock for deterministic testing
#[cfg(any(test, feature = "testing"))]
#[derive(Debug, Clone)]
pub struct TestClock {
    time: Arc<AtomicU64>,
}

// Methods for TestClock
#[cfg(any(test, feature = "testing"))]
#[allow(dead_code)]
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
}

// Implement Clock trait for TestClock
#[cfg(any(test, feature = "testing"))]
impl Clock for TestClock {
    fn now(&self) -> u64 {
        self.time.load(Ordering::Relaxed)
    }
}
