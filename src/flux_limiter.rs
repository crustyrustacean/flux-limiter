// src/flux_limiter.rs

// flux-limiter: A rate limiter based on the Generic Cell Rate Algorithm (GCRA).

// dependencies
use crate::clock::{Clock, MonotonicClock};
use crate::config::FluxLimiterConfig;
use crate::errors::FluxLimiterError;
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use std::borrow::{Borrow, ToOwned};
use std::hash::Hash;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

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
/// C is the clock type, defaulting to [`MonotonicClock`], which is immune to OS
/// clock steps. Pass [`crate::SystemClock`] explicitly for wall-clock time.
/// Each client's state is a single atomic TAT. Once a client is known, a request
/// updates it with a compare-and-swap and takes no lock at all; the map's shard
/// lock is only taken when a client is seen for the first time.
#[derive(Debug)]
pub struct FluxLimiter<T, C = MonotonicClock>
where
    T: Hash + Eq + Clone,
    C: Clock,
{
    rate_nanos: u64,
    tolerance_nanos: u64,
    client_state: Arc<DashMap<T, AtomicU64>>,
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
        // `as u64` on a float is a *saturating* cast: it clamps out-of-range values
        // instead of failing. Every bound below is therefore checked in f64, before
        // any cast, so a cast can never be the thing that silently "succeeds".
        let rate_nanos_f64 = 1_000_000_000.0 / rate_per_second;
        // Below 1.0 the interval would truncate to zero (no limit at all); at or
        // above 2^64 it would saturate to u64::MAX and overflow on the first request.
        if !(1.0..u64::MAX as f64).contains(&rate_nanos_f64) {
            // Unreachable while validate() enforces its bounds; kept so that neither
            // a zero nor a saturated interval can be constructed, whatever the path.
            return Err(FluxLimiterError::InvalidRate);
        }
        let rate_nanos = rate_nanos_f64 as u64;

        // `u64::MAX as f64` rounds up to exactly 2^64, the first value that does
        // not fit, so this is an exclusive upper bound on a representable tolerance.
        let tolerance_nanos_f64 = burst_capacity * rate_nanos as f64;
        if tolerance_nanos_f64 >= u64::MAX as f64 {
            return Err(FluxLimiterError::InvalidBurst);
        }
        let tolerance_nanos = tolerance_nanos_f64 as u64;

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
    ///
    /// The key may be borrowed, like the key of [`Self::check_request_ref`]: for a
    /// `String`-keyed limiter, pass a `&str`.
    pub fn contains_client<Q>(&self, client_id: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.client_state.contains_key(client_id)
    }

    /// Decides whether a request from `client_id` should be allowed, and updates the
    /// client's state if it is.
    ///
    /// This takes the key by value. If the caller holds a borrowed key (a `&str` for a
    /// `String`-keyed limiter, say), [`Self::check_request_ref`] avoids cloning it on
    /// every request.
    pub fn check_request(&self, client_id: T) -> Result<FluxLimiterDecision, FluxLimiterError> {
        let current_time_nanos = self.clock.now()?;

        // Fast path: the client is already known. `client_id` is only ever borrowed
        // here, exactly as in `check_request_ref`: a request for a known client
        // never clones or moves the key.
        if let Some(tat) = self.client_state.get(&client_id) {
            match self.try_advance(&tat, current_time_nanos) {
                Some(decision) => return Ok(decision),
                None => return self.check_request_owned_slow(client_id, current_time_nanos),
            }
        }

        // The client is new (or the insert path is needed). Storing a key needs an
        // owned one, and the entry API consumes it, so a copy is made here and only
        // here: the same single allocation this path always made.
        self.check_request_new_owned(client_id, current_time_nanos)
    }

    /// The slow path for an owned key: a known client that keeps losing the
    /// compare-and-swap race. The retries are lookups that borrow the key; only the
    /// locked last resort consumes it.
    fn check_request_owned_slow(
        &self,
        client_id: T,
        current_time_nanos: u64,
    ) -> Result<FluxLimiterDecision, FluxLimiterError> {
        const MAX_SPINS: usize = 64;

        for _ in 0..MAX_SPINS {
            if let Some(tat) = self.client_state.get(&client_id)
                && let Some(decision) = self.try_advance(&tat, current_time_nanos)
            {
                return Ok(decision);
            }
        }
        self.decide_under_lock(client_id, current_time_nanos)
    }

    /// The first request from a client, for an owned key. The key is stored by value.
    /// The entry API takes the key by value and gives it back only as a reference, so
    /// a second copy is kept for the lost-insert-race retry; that copy, like the
    /// insert itself, happens once in the lifetime of a client.
    fn check_request_new_owned(
        &self,
        client_id: T,
        current_time_nanos: u64,
    ) -> Result<FluxLimiterDecision, FluxLimiterError> {
        match self.client_state.entry(client_id.clone()) {
            Entry::Vacant(vacant) => {
                // A client's first request is always allowed.
                let new_tat_nanos = current_time_nanos.saturating_add(self.rate_nanos);
                vacant.insert(AtomicU64::new(new_tat_nanos));

                Ok(FluxLimiterDecision {
                    allowed: true,
                    retry_after_seconds: None,
                    remaining_capacity: Some(
                        self.calculate_remaining_capacity(current_time_nanos, new_tat_nanos),
                    ),
                    reset_time_nanos: new_tat_nanos,
                })
            }
            Entry::Occupied(entry) => {
                // Lost the insert race. Drop the entry guard before deciding so the
                // shard is not held across the retry.
                drop(entry);
                self.check_request_owned_slow(client_id, current_time_nanos)
            }
        }
    }

    /// Decides whether a request from the client identified by `key` should be allowed,
    /// and updates the client's state if it is.
    ///
    /// This is [`Self::check_request`] for a borrowed key. The key is hashed and compared
    /// without being owned, so a known client costs no allocation: for a `String`-keyed
    /// limiter a caller can pass a `&str` instead of cloning a `String` per request.
    ///
    /// The key is copied once, and only once, on the request that first adds the client.
    ///
    /// # Examples
    ///
    /// ```
    /// use flux_limiter::{FluxLimiter, FluxLimiterConfig, MonotonicClock};
    ///
    /// # fn main() -> Result<(), flux_limiter::FluxLimiterError> {
    /// let config = FluxLimiterConfig::new(10.0, 5.0);
    /// // The limiter is keyed by `String`, but the check takes a `&str`:
    /// let limiter: FluxLimiter<String> = FluxLimiter::with_config(config, MonotonicClock::new())?;
    /// let decision = limiter.check_request_ref("user_123")?;
    /// assert!(decision.allowed);
    /// # Ok(())
    /// # }
    /// ```
    pub fn check_request_ref<Q>(&self, key: &Q) -> Result<FluxLimiterDecision, FluxLimiterError>
    where
        T: Borrow<Q>,
        Q: Hash + Eq + ToOwned<Owned = T> + ?Sized,
    {
        let current_time_nanos = self.clock.now()?;
        self.check_request_ref_impl(key, current_time_nanos, || key.to_owned())
    }

    /// The one implementation behind both entry points.
    ///
    /// `key` is only ever looked up, never stored. `make_owned` produces the key that
    /// is stored, and is called only on a path that stores the client: a request for
    /// a known client never calls it.
    ///
    /// `to_owned` rebuilds that key from the borrowed one. It is needed on one path
    /// only: a first request that loses the insert race retries as a known client, and
    /// if that retry reaches the locked last resort it has to store the key after all.
    fn check_request_ref_impl<Q>(
        &self,
        key: &Q,
        current_time_nanos: u64,
        make_owned: impl FnOnce() -> T,
    ) -> Result<FluxLimiterDecision, FluxLimiterError>
    where
        T: Borrow<Q>,
        Q: Hash + Eq + ToOwned<Owned = T> + ?Sized,
    {
        // Fast path: the client is already known. Decide from the TAT we read, then
        // publish the new TAT only if it is still the one we decided on. This is a
        // lock-free read-modify-write per client, so requests for the same client
        // retry instead of queueing behind the shard's exclusive lock, and requests
        // for different clients never contend with each other.
        //
        // Each attempt takes a fresh read guard and releases it before retrying, so
        // a retrying request never holds the shard and blocks a concurrent insert.
        //
        // Interaction with `cleanup_stale_clients`: cleanup evicts a client only once
        // its TAT is older than the staleness cutoff, i.e. after a long idle period.
        // A request that read the TAT just before such an eviction may still land its
        // compare-and-swap on the orphaned cell. That can only ever under-count (the
        // request is allowed, but its successor sees a fresh client), never over-allow,
        // because the fresh client starts from a TAT of now + one interval.
        if let Some(tat) = self.client_state.get(key) {
            match self.try_advance(&tat, current_time_nanos) {
                Some(decision) => return Ok(decision),
                None => return self.check_request_known(key, current_time_nanos, make_owned),
            }
        }
        self.check_request_new(key, current_time_nanos, make_owned)
    }

    /// One lock-free attempt on a known client. Returns `None` if another request
    /// advanced the TAT between our read and our write, so the caller can retry.
    fn try_advance(&self, tat: &AtomicU64, current_time_nanos: u64) -> Option<FluxLimiterDecision> {
        let previous_tat = tat.load(Ordering::Relaxed);
        let decision = self.decide(previous_tat, current_time_nanos);
        if decision.allowed
            && tat
                .compare_exchange(
                    previous_tat,
                    decision.reset_time_nanos,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .is_err()
        {
            return None;
        }
        Some(decision)
    }

    /// Retries the lock-free path for a client that has lost a compare-and-swap race.
    /// A request that keeps losing could in principle spin forever on a pathological
    /// key; after a fixed number of attempts it takes the shard's write lock once and
    /// settles the decision deterministically instead.
    ///
    /// The retries are lookups, so they use the borrowed key. Only the locked last
    /// resort needs an owned one, and `make_owned` is called there and nowhere else.
    fn check_request_known<Q>(
        &self,
        key: &Q,
        current_time_nanos: u64,
        make_owned: impl FnOnce() -> T,
    ) -> Result<FluxLimiterDecision, FluxLimiterError>
    where
        T: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        const MAX_SPINS: usize = 64;

        for _ in 0..MAX_SPINS {
            if let Some(tat) = self.client_state.get(key)
                && let Some(decision) = self.try_advance(&tat, current_time_nanos)
            {
                return Ok(decision);
            }
        }
        self.decide_under_lock(make_owned(), current_time_nanos)
    }

    /// First request from a client: insert its initial TAT under the entry lock.
    /// If another request inserted this client first, it is no longer new, so we
    /// fall through to the normal path on the value that request stored.
    ///
    /// This is where the borrowed key becomes an owned one: the entry API must be
    /// given an owned key to store, and DashMap has no borrowed entry.
    fn check_request_new<Q>(
        &self,
        key: &Q,
        current_time_nanos: u64,
        make_owned: impl FnOnce() -> T,
    ) -> Result<FluxLimiterDecision, FluxLimiterError>
    where
        T: Borrow<Q>,
        Q: Hash + Eq + ToOwned<Owned = T> + ?Sized,
    {
        match self.client_state.entry(make_owned()) {
            Entry::Vacant(vacant) => {
                // A client's first request is always allowed.
                let new_tat_nanos = current_time_nanos.saturating_add(self.rate_nanos);
                vacant.insert(AtomicU64::new(new_tat_nanos));

                Ok(FluxLimiterDecision {
                    allowed: true,
                    retry_after_seconds: None,
                    remaining_capacity: Some(
                        self.calculate_remaining_capacity(current_time_nanos, new_tat_nanos),
                    ),
                    reset_time_nanos: new_tat_nanos,
                })
            }
            Entry::Occupied(entry) => {
                // Lost the insert race: another request stored this client between our
                // lookup and our insert. Drop the entry guard before deciding so the
                // shard is not held across the retry. The client is known now, so the
                // retry is lookups; should it reach the locked last resort it builds
                // the key afresh from the borrowed one.
                drop(entry);
                self.check_request_known(key, current_time_nanos, || key.to_owned())
            }
        }
    }

    /// Last-resort path, taken under the shard's write lock. Only reachable after
    /// many failed compare-and-swaps on one key.
    ///
    /// The write lock keeps other locked writers and `cleanup_stale_clients` out,
    /// but it does not stop a concurrent lock-free compare-and-swap on the same
    /// cell, so the store is itself a compare-and-swap: if it fails we simply
    /// retry, and the decision is recomputed from the value that won.
    fn decide_under_lock(
        &self,
        client_id: T,
        current_time_nanos: u64,
    ) -> Result<FluxLimiterDecision, FluxLimiterError> {
        match self.client_state.entry(client_id) {
            Entry::Occupied(entry) => {
                let tat = entry.get();
                loop {
                    let previous_tat = tat.load(Ordering::Relaxed);
                    let decision = self.decide(previous_tat, current_time_nanos);
                    if !decision.allowed
                        || tat
                            .compare_exchange(
                                previous_tat,
                                decision.reset_time_nanos,
                                Ordering::Relaxed,
                                Ordering::Relaxed,
                            )
                            .is_ok()
                    {
                        return Ok(decision);
                    }
                }
            }
            // The client was evicted by cleanup while we were retrying: it is new again.
            Entry::Vacant(vacant) => {
                let new_tat_nanos = current_time_nanos.saturating_add(self.rate_nanos);
                vacant.insert(AtomicU64::new(new_tat_nanos));
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

    /// The GCRA decision for a client whose current TAT is `previous_tat`.
    /// Pure: it reads no shared state and writes none.
    fn decide(&self, previous_tat: u64, current_time_nanos: u64) -> FluxLimiterDecision {
        let earliest_allowed = previous_tat.saturating_sub(self.tolerance_nanos);

        if current_time_nanos >= earliest_allowed {
            let new_tat_nanos = current_time_nanos
                .max(previous_tat)
                .saturating_add(self.rate_nanos);
            FluxLimiterDecision {
                allowed: true,
                retry_after_seconds: None,
                remaining_capacity: Some(
                    self.calculate_remaining_capacity(current_time_nanos, new_tat_nanos),
                ),
                reset_time_nanos: new_tat_nanos,
            }
        } else {
            let retry_after_nanos = earliest_allowed.saturating_sub(current_time_nanos);
            FluxLimiterDecision {
                allowed: false,
                retry_after_seconds: Some(retry_after_nanos as f64 / 1_000_000_000.0),
                remaining_capacity: Some(0.0),
                reset_time_nanos: previous_tat,
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
        self.client_state.retain(|_, tat| {
            // Saturating keeps a client whose TAT + tolerance would overflow: a wrapped
            // value could compare as "old" and evict a live client, whereas u64::MAX is
            // always newer than the cutoff.
            tat.load(Ordering::Relaxed)
                .saturating_add(self.tolerance_nanos)
                > current_time_nanos.saturating_sub(max_stale_nanos)
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
    /// When the rate limit window resets, in nanoseconds since the Unix epoch.
    ///
    /// With the default [`MonotonicClock`] this is derived from elapsed time: it
    /// tracks the OS clock to within microseconds for as long as that clock is
    /// not stepped, and it never goes backwards.
    pub reset_time_nanos: u64,
}
