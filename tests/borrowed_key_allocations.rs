//! `check_request_ref` does not allocate for a client that is already known.
//!
//! The claim is proved by counting allocations with a global allocator. The owned-key
//! `check_request` is measured the same way as a control: it must allocate, or the
//! counter is not working and the first assertion would be vacuous.
//!
//! The counter is per thread, not per process. The harness runs tests on several
//! threads at once, so a process-wide count would include other tests' allocations and
//! the harness's own, and would fail intermittently. Counting only the calling thread
//! measures exactly what the limiter allocates, so plain `cargo test` needs no special
//! flag and no lock.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::Arc;

use flux_limiter::{FluxLimiter, FluxLimiterConfig, MonotonicClock};

struct Counting;

thread_local! {
    /// Allocations made by the thread that is reading it. Other threads have their own.
    static ON_THIS_THREAD: Cell<usize> = const { Cell::new(0) };
}

fn on_this_thread() -> usize {
    ON_THIS_THREAD.with(Cell::get)
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // `try_with` because this may run while the thread-local is being destroyed.
        let _ = ON_THIS_THREAD.try_with(|count| count.set(count.get() + 1));
        // Safety: this is a direct forwarding to the system allocator.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // Safety: `ptr` was allocated by `System` with `layout`.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static A: Counting = Counting;

/// Allocations the current thread makes while `f` runs.
fn allocations_during<T>(f: impl FnOnce() -> T) -> (usize, T) {
    // Touch the counter before taking the baseline. Its storage is created on first
    // use, and creating it allocates twice; that is the counter's cost, not the
    // limiter's, so it must not land inside a measured region.
    let before = on_this_thread();
    let out = f();
    (on_this_thread() - before, out)
}

fn limiter() -> Arc<FluxLimiter<String>> {
    Arc::new(
        FluxLimiter::with_config(FluxLimiterConfig::new(1e9, 1e9), MonotonicClock::new()).unwrap(),
    )
}

/// Makes the client known, then measures `checks` further requests. Every request is
/// allowed (rate 1e9/s), so each measured check takes the path that writes state.
fn allocations_for_known_client(checks: usize, make: impl Fn(&FluxLimiter<String>)) -> usize {
    let limiter = limiter();
    make(&limiter);
    let (allocs, ()) = allocations_during(|| {
        for _ in 0..checks {
            make(&limiter);
        }
    });
    allocs
}

#[test]
fn borrowed_key_does_not_allocate_for_a_known_client() {
    let allocs = allocations_for_known_client(1_000, |l| {
        l.check_request_ref("client-1").unwrap();
    });
    assert_eq!(
        allocs, 0,
        "check_request_ref made {allocs} allocations over 1000 checks on a known client"
    );
}

#[test]
fn owned_key_allocates_once_per_check_as_the_control() {
    // The caller must own a String to call this, so it builds one per request. That is
    // the cost `check_request_ref` removes. If this ever stopped allocating, the zero
    // in the test above would prove nothing.
    let allocs = allocations_for_known_client(1_000, |l| {
        l.check_request(String::from("client-1")).unwrap();
    });
    assert!(
        allocs >= 1_000,
        "expected at least one allocation per check, saw {allocs}"
    );
}

#[test]
fn a_new_client_is_inserted_once_and_then_never_allocates() {
    let limiter = limiter();
    // First request: the client is inserted, which must own its key.
    let (first, decision) = allocations_during(|| limiter.check_request_ref("client-3").unwrap());
    assert!(decision.allowed);
    assert!(first >= 1, "inserting a new client must allocate the entry");
    // Every later request through the borrowed key: no allocation.
    let (rest, ()) = allocations_during(|| {
        for _ in 0..500 {
            limiter.check_request_ref("client-3").unwrap();
        }
    });
    assert_eq!(
        rest, 0,
        "a known client made {rest} allocations over 500 checks"
    );
    assert_eq!(limiter.client_count(), 1);
    assert!(limiter.contains_client("client-3"));
}
