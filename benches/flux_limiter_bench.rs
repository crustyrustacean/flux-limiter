// benches/flux_limiter_bench.rs

use std::hint::black_box;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
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

/// Contended throughput, measured per request rather than per batch.
///
/// Worker threads are spawned once for the whole benchmark and parked on a
/// channel between rounds, so no thread start-up is ever inside the timed
/// region. Each timed round releases every worker at once, which is the
/// condition under which same-client requests actually collide.
///
/// The rate is 1e9/s so that every request is allowed and therefore every
/// request writes. A denied request takes a path that never touches the
/// client's state, so a benchmark dominated by denials measures nothing about
/// contention.
///
/// `hot` has every thread hit one client, the worst case for same-client
/// contention. `spread` gives each thread its own clients, the common case.
fn bench_contention(c: &mut Criterion) {
    const THREADS: usize = 4;
    const PER_ROUND: usize = 1_000;

    for (name, shared) in [
        ("contention_hot_key_u64", true),
        ("contention_spread_keys_u64", false),
    ] {
        let limiter = Arc::new(
            FluxLimiter::with_config(FluxLimiterConfig::new(1e9, 1e9), SystemClock).unwrap(),
        );

        // Worker t runs rounds until its start channel closes. Each round it is
        // told how many requests to make, then signals completion on the shared
        // `done` channel, which the timing loop drains.
        let (done_tx, done_rx) = mpsc::channel::<()>();
        let mut starts = Vec::with_capacity(THREADS);
        let mut workers = Vec::with_capacity(THREADS);
        for t in 0..THREADS {
            let limiter = Arc::clone(&limiter);
            let done_tx = done_tx.clone();
            let (start_tx, start_rx) = mpsc::channel::<usize>();
            starts.push(start_tx);
            workers.push(thread::spawn(move || {
                for count in start_rx {
                    for i in 0..count {
                        // In the spread case each worker owns a distinct key
                        // range, so no two workers ever touch the same client.
                        // They can still hash into the same map shard, which is
                        // inherent to sharding by hash.
                        let key = if shared {
                            1u64
                        } else {
                            (t * PER_ROUND + i) as u64
                        };
                        black_box(limiter.check_request(key).unwrap());
                    }
                    done_tx.send(()).unwrap();
                }
            }));
        }
        // The timing loop is the only sender-holder now; keep one for itself.
        drop(done_tx);

        let mut group = c.benchmark_group(name);
        group.throughput(Throughput::Elements((THREADS * PER_ROUND) as u64));
        group.bench_function("4_threads", |b| {
            b.iter_custom(|iters| {
                let mut elapsed = Duration::ZERO;
                for _ in 0..iters {
                    // Parked workers wake in turn; the round is timed from the
                    // first release to the last completion.
                    let start = Instant::now();
                    for start_tx in &starts {
                        start_tx.send(PER_ROUND).unwrap();
                    }
                    for _ in 0..THREADS {
                        done_rx.recv().unwrap();
                    }
                    elapsed += start.elapsed();
                }
                elapsed
            })
        });
        group.finish();

        // Closing the start channels ends each worker's loop.
        drop(starts);
        for w in workers {
            w.join().unwrap();
        }
    }
}

criterion_group!(
    benches,
    bench_single_client_u64,
    bench_single_client_string,
    bench_contended_threads_u64,
    bench_contention
);
criterion_main!(benches);
