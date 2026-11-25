// src/clock.rs

// clock module definition and implementations

// dependencies
use std::fmt;
use std::sync::{Arc, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

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

impl fmt::Display for ClockError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ClockError::SystemTimeError => write!(f, "System time error"),
        }
    }
}

impl std::error::Error for ClockError {}

/// A clock that reads the operating system's wall-clock time.
///
/// Returns the current time in nanoseconds since the Unix epoch. This is the
/// **opt-in** clock, for callers who need readings that match the OS clock
/// exactly. The default clock is [`MonotonicClock`].
///
/// # Known limitation: wall-clock time
///
/// Wall-clock time is not monotonic. NTP corrections, manual changes and
/// suspend/resume can move it backwards. A backwards step makes every client's
/// stored state look as though it is in the future, so requests are denied,
/// with no error reported, until the OS clock catches back up. This is not a
/// source of [`ClockError`]: reading the clock still succeeds.
///
/// Returns [`ClockError::SystemTimeError`] if the OS clock is before the Unix
/// epoch.
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

/// The default clock: monotonic, and anchored to the Unix epoch.
///
/// The rate limiter only needs *elapsed* time, never the OS clock's notion of
/// "now". This clock reads the OS clock exactly once, on its first use, to
/// record an anchor. Every later reading is `anchor + elapsed`, so it:
///
/// - never goes backwards, whatever the OS clock does;
/// - advances by real elapsed time, so capacity refills correctly even while
///   the OS clock is wrong;
/// - is still a Unix-epoch timestamp in nanoseconds, within microseconds of the
///   OS clock for as long as that clock is not stepped. That keeps
///   [`crate::FluxLimiterDecision::reset_time_nanos`] usable in headers such as
///   `X-RateLimit-Reset` without conversion.
///
/// After the OS clock is stepped, readings differ from it by the size of the
/// step, by design. A forward step is invisible to this clock.
///
/// This clock never returns an error, because [`Instant::elapsed`] cannot fail.
///
/// `MonotonicClock` can be written bare wherever a clock is taken, exactly like
/// [`SystemClock`]: `FluxLimiter::with_config(config, MonotonicClock)`. Clones
/// share one anchor, so a cloned limiter and its original see the same time.
/// Two independently constructed clocks have independent anchors that agree to
/// within microseconds of each other.
#[derive(Debug, Clone, Default)]
pub struct MonotonicClock {
    /// Recorded on the first call to `now()`. The `OnceLock` makes concurrent
    /// first calls agree on a single anchor.
    anchor: Arc<OnceLock<Anchor>>,
    /// Where elapsed time comes from. The real path is a direct `Instant::elapsed()`
    /// with no indirection; the injected variant exists only for tests.
    source: ElapsedSource,
}

/// What the anchor records: the point in elapsed time, and the wall-clock
/// reading taken at (effectively) the same moment.
#[derive(Debug, Clone, Copy)]
struct Anchor {
    elapsed_at_anchor: u64,
    epoch_at_anchor: u64,
}

impl MonotonicClock {
    /// Constructs a clock whose anchor is recorded on its first use.
    ///
    /// `MonotonicClock::default()` is equivalent, and the bare value
    /// `MonotonicClock` works anywhere a clock is taken.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Clock for MonotonicClock {
    fn now(&self) -> Result<u64, ClockError> {
        let anchor = self.anchor.get_or_init(|| Anchor {
            elapsed_at_anchor: self.source.elapsed_nanos(),
            epoch_at_anchor: self.source.epoch_nanos(),
        });
        let since = self
            .source
            .elapsed_nanos()
            .saturating_sub(anchor.elapsed_at_anchor);
        Ok(anchor.epoch_at_anchor.saturating_add(since))
    }
}

/// Not part of the public API.
///
/// Hidden support for [`MonotonicClock`]. It is a module, rather than private
/// items, only because integration tests are a separate crate and need to reach
/// it by path. Nothing here is covered by the crate's stability guarantees.
#[doc(hidden)]
pub mod hidden {
    use super::*;

    /// A monotonic source of elapsed nanoseconds plus one epoch reading.
    ///
    /// `elapsed_nanos` must never return a value lower than one it returned
    /// earlier on the same source.
    #[doc(hidden)]
    pub trait ElapsedSource: Send + Sync {
        fn elapsed_nanos(&self) -> u64;
        fn epoch_nanos(&self) -> u64;
    }

    #[doc(hidden)]
    impl super::MonotonicClock {
        /// Test-only: build a clock whose time comes from `source`.
        #[doc(hidden)]
        pub fn with_source(source: Arc<dyn ElapsedSource>) -> super::MonotonicClock {
            super::MonotonicClock {
                anchor: Arc::new(OnceLock::new()),
                source: super::ElapsedSource::Injected(source),
            }
        }

        /// Test-only: identify the anchor, to check that clones share one.
        #[doc(hidden)]
        pub fn anchor_id(&self) -> usize {
            Arc::as_ptr(&self.anchor) as usize
        }
    }
}

/// Where elapsed time comes from.
#[derive(Clone)]
enum ElapsedSource {
    /// The real source: `Instant::elapsed()` for time, the OS clock read once
    /// for the epoch. The origin is fixed when the clock is constructed.
    Real {
        origin: Instant,
        state: OnceLock<u64>,
    },
    Injected(Arc<dyn hidden::ElapsedSource>),
}

impl ElapsedSource {
    fn elapsed_nanos(&self) -> u64 {
        match self {
            ElapsedSource::Real { origin, .. } => origin.elapsed().as_nanos() as u64,
            ElapsedSource::Injected(s) => s.elapsed_nanos(),
        }
    }

    fn epoch_nanos(&self) -> u64 {
        match self {
            // Read the OS clock at most once per clock, whichever thread gets here
            // first. Saturating at 0 keeps the clock monotonic if the OS clock is
            // somehow before the epoch; there is no valid reading to return then.
            ElapsedSource::Real { state, .. } => *state.get_or_init(|| {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_nanos() as u64)
                    .unwrap_or(0)
            }),
            ElapsedSource::Injected(s) => s.epoch_nanos(),
        }
    }
}

// Manual impl: an injected source is a trait object that need not be `Debug`,
// and it only ever appears in tests.
impl fmt::Debug for ElapsedSource {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ElapsedSource::Real { .. } => f.pad("Real"),
            ElapsedSource::Injected(_) => f.pad("Injected"),
        }
    }
}

impl Default for ElapsedSource {
    fn default() -> Self {
        ElapsedSource::Real {
            origin: Instant::now(),
            state: OnceLock::new(),
        }
    }
}
