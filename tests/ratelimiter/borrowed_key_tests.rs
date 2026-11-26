//! `check_request_ref` behaves exactly like `check_request`.
//!
//! These tests check behaviour, so they share the main test binary. The tests that
//! count allocations are in `tests/borrowed_key_allocations.rs`, in a binary of their
//! own: a process-wide allocation counter cannot share a binary with a parallel runner.

use crate::setup_limiter;
use flux_limiter::{FluxLimiter, FluxLimiterConfig, MonotonicClock};

fn limiter() -> FluxLimiter<String> {
    FluxLimiter::with_config(FluxLimiterConfig::new(1e9, 1e9), MonotonicClock::new()).unwrap()
}

#[test]
fn both_entry_points_agree_on_every_decision() {
    // One limiter, alternating entry points, same key. At 1e9/s every request is
    // allowed, whichever way the key is passed.
    let limiter = limiter();
    let key = String::from("client-2");
    let mut allowed_owned = 0usize;
    let mut allowed_borrowed = 0usize;
    for _ in 0..2_000 {
        let a = limiter.check_request(key.clone()).unwrap();
        let b = limiter.check_request_ref("client-2").unwrap();
        assert!(a.remaining_capacity.unwrap_or(0.0) >= 0.0);
        assert!(b.remaining_capacity.unwrap_or(0.0) >= 0.0);
        if a.allowed {
            assert!(a.retry_after_seconds.is_none());
            allowed_owned += 1;
        }
        if b.allowed {
            assert!(b.retry_after_seconds.is_none());
            allowed_borrowed += 1;
        }
    }
    assert_eq!((allowed_owned, allowed_borrowed), (2_000, 2_000));
}

#[test]
fn a_borrowed_key_of_a_different_type_resolves_the_same_client() {
    // `&str` and `&String` are different `Q`s; both must address one client, and the
    // owned entry point must see that same client.
    let limiter = limiter();
    assert!(limiter.check_request_ref("client-4").unwrap().allowed);
    let owned = String::from("client-4");
    assert!(limiter.check_request_ref(&owned).unwrap().allowed);
    assert!(limiter.check_request(owned.clone()).unwrap().allowed);
    assert_eq!(limiter.client_count(), 1, "three checks, one client");
}

#[test]
fn a_denied_request_reports_the_same_retry_time_either_way() {
    // A limiter that can deny: 1 per second, burst 1, on a clock that does not move.
    let (limiter, clock) = setup_limiter(1.0, 1.0, 100.0);

    // A burst of 1 allows two requests in an instant: the first, plus one within the
    // tolerance. Spend both, one through each entry point.
    assert!(limiter.check_request_ref("client-5").unwrap().allowed);
    assert!(
        limiter
            .check_request(String::from("client-5"))
            .unwrap()
            .allowed
    );

    // Both entry points now report a denial against the same TAT at the same instant,
    // so every field of the decision must agree exactly.
    let by_ref = limiter.check_request_ref("client-5").unwrap();
    let owned = limiter.check_request(String::from("client-5")).unwrap();
    assert!(!by_ref.allowed);
    assert!(!owned.allowed);
    assert_eq!(by_ref.retry_after_seconds, owned.retry_after_seconds);
    assert_eq!(by_ref.retry_after_seconds, Some(1.0));
    assert_eq!(by_ref.reset_time_nanos, owned.reset_time_nanos);
    assert_eq!(by_ref.remaining_capacity, owned.remaining_capacity);

    // After one full interval the client has earned exactly one more request.
    clock.advance(1.0);
    assert!(limiter.check_request_ref("client-5").unwrap().allowed);
    assert!(!limiter.check_request_ref("client-5").unwrap().allowed);
}

#[test]
fn a_borrowed_key_can_be_any_hashable_type() {
    // The bound is `Q: Hash + Eq + ToOwned<Owned = T>`: a `u64` key passed as `&u64`.
    let limiter =
        FluxLimiter::with_config(FluxLimiterConfig::new(1e9, 1e9), MonotonicClock::new()).unwrap();
    assert!(limiter.check_request_ref(&7u64).unwrap().allowed);
    assert!(limiter.check_request(7u64).unwrap().allowed);
    assert_eq!(limiter.client_count(), 1);
}
