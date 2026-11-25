// benches/flux_limiter_bench.rs

use std::hint::black_box;
use std::sync::Arc;
use std::thread;

use criterion::{Criterion, criterion_group, criterion_main};
use flux_limiter::{FluxLimiter, FluxLimiterConfig, SystemClock};

fn bench_single_client_u64(c: &mut Criterion) {
    let config = FluxLimiterConfig::new(10_000.0, 100.0);
    let limiter = FluxLimiter::with_config(config, SystemClock).unwrap();

    // Seed the client so we benchmark the Entry::Occupied hot path
    let _ = limiter.check_request(1u64);

    c.bench_function("single_client_u64", |b| {
        b.iter(|| {
            // black_box prevents the compiler from optimizing our call away
            limiter.check_request(black_box(1u64)).unwrap()
        })
    });
}

fn bench_single_client_string(c: &mut Criterion) {
    let config = FluxLimiterConfig::new(10_000.0, 100.0);
    let limiter = FluxLimiter::with_config(config, SystemClock).unwrap();
    let client_id = String::from("user_12345");
    let _ = limiter.check_request(client_id.clone());

    c.bench_function("single_client_string_clone", |b| {
        b.iter(|| limiter.check_request(black_box(client_id.clone())).unwrap())
    });
}

fn bench_contended_threads_u64(c: &mut Criterion) {
    let config = FluxLimiterConfig::new(100_000.0, 1000.0);
    let limiter = Arc::new(FluxLimiter::with_config(config, SystemClock).unwrap());
    let _ = limiter.check_request(1u64);

    c.bench_function("contended_4_threads_u64", |b| {
        b.iter(|| {
            thread::scope(|s| {
                for _ in 0..4 {
                    let lim = Arc::clone(&limiter);
                    s.spawn(move || {
                        for _ in 0..250 {
                            lim.check_request(black_box(1u64)).unwrap();
                        }
                    });
                }
            });
        })
    });
}

criterion_group!(
    benches,
    bench_single_client_u64,
    bench_single_client_string,
    bench_contended_threads_u64
);
criterion_main!(benches);
