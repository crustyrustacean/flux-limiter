# Future Extensibility

Planned architecture enhancements and extension points.

## Planned Features

### 1. Async Support

Add async variants of key methods for better integration with async runtimes.

```rust
impl<T, C> FluxLimiter<T, C>
where
    T: Hash + Eq + Clone + Send + Sync,
    C: Clock + Send + Sync,
{
    pub async fn check_request_async(
        &self,
        client_id: T,
    ) -> Result<FluxLimiterDecision, FluxLimiterError> {
        // Non-blocking implementation
        // Async clock sources
        // Future-based cleanup
    }

    pub async fn cleanup_stale_clients_async(
        &self,
        stale_threshold_nanos: u64,
    ) -> Result<(), FluxLimiterError> {
        // Async cleanup without blocking
    }
}
```

**Benefits**:
- Better integration with Tokio/async-std
- Non-blocking I/O for distributed backends
- Async clock sources

**Implementation Strategy**:
- Feature flag: `async`
- Separate module: `flux_limiter::async`
- Maintain backward compatibility

### 2. Persistence Layer

Support for persisting rate limit state across restarts.

```rust
#[async_trait]
pub trait StateStore: Send + Sync {
    async fn load_state(
        &self,
        client_id: &str,
    ) -> Result<Option<u64>, StoreError>;

    async fn save_state(
        &self,
        client_id: &str,
        tat: u64,
    ) -> Result<(), StoreError>;

    async fn cleanup_stale(
        &self,
        threshold: u64,
    ) -> Result<usize, StoreError>;
}
```

**Implementations**:

#### Redis Backend

```rust
pub struct RedisStateStore {
    client: redis::Client,
    key_prefix: String,
}

impl StateStore for RedisStateStore {
    async fn load_state(&self, client_id: &str) -> Result<Option<u64>, StoreError> {
        let key = format!("{}:{}", self.key_prefix, client_id);
        // Load TAT from Redis
    }

    async fn save_state(&self, client_id: &str, tat: u64) -> Result<(), StoreError> {
        let key = format!("{}:{}", self.key_prefix, client_id);
        // Save TAT to Redis with TTL
    }
}
```

#### Database Backend

```rust
pub struct DatabaseStateStore {
    pool: sqlx::PgPool,
}

impl StateStore for DatabaseStateStore {
    async fn load_state(&self, client_id: &str) -> Result<Option<u64>, StoreError> {
        sqlx::query_scalar("SELECT tat FROM rate_limits WHERE client_id = $1")
            .bind(client_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(Into::into)
    }
}
```

**Benefits**:
- State survives restarts
- Distributed rate limiting
- Shared state across instances

**Implementation Strategy**:
- Feature flags: `redis`, `postgres`, etc.
- Trait-based design
- Fallback to in-memory

### 3. Observability Integration

Built-in metrics and tracing support.

```rust
pub trait RateLimiterMetrics: Send + Sync {
    fn record_decision(
        &self,
        client_id: &str,
        allowed: bool,
        latency: Duration,
    );

    fn record_error(&self, error: &FluxLimiterError);

    fn record_cleanup(&self, removed: usize, duration: Duration);
}
```

**Implementations**:

#### Prometheus Metrics

```rust
pub struct PrometheusMetrics {
    requests_total: IntCounterVec,
    requests_allowed: IntCounterVec,
    requests_denied: IntCounterVec,
    check_duration: HistogramVec,
    clock_errors: IntCounter,
}

impl RateLimiterMetrics for PrometheusMetrics {
    fn record_decision(&self, client_id: &str, allowed: bool, latency: Duration) {
        self.requests_total.with_label_values(&[client_id]).inc();

        if allowed {
            self.requests_allowed.with_label_values(&[client_id]).inc();
        } else {
            self.requests_denied.with_label_values(&[client_id]).inc();
        }

        self.check_duration
            .with_label_values(&[client_id])
            .observe(latency.as_secs_f64());
    }
}
```

#### OpenTelemetry Tracing

```rust
pub struct OpenTelemetryMetrics {
    meter: Meter,
}

impl RateLimiterMetrics for OpenTelemetryMetrics {
    fn record_decision(&self, client_id: &str, allowed: bool, latency: Duration) {
        let span = tracing::span!(
            tracing::Level::INFO,
            "rate_limit_check",
            client_id = %client_id,
            allowed = allowed,
            latency_us = latency.as_micros() as i64,
        );
        let _enter = span.enter();
    }
}
```

**Benefits**:
- Built-in monitoring
- Standard metrics
- Distributed tracing

**Implementation Strategy**:
- Feature flags: `prometheus`, `opentelemetry`
- Optional trait implementations
- Zero-cost when disabled

### 4. Alternative Algorithms

Support for multiple rate limiting algorithms.

```rust
pub trait RateLimitAlgorithm<T>: Send + Sync {
    fn check_request(
        &self,
        client_id: &T,
        current_time: u64,
    ) -> FluxLimiterDecision;

    fn cleanup_stale(
        &self,
        threshold: u64,
    ) -> Result<usize, FluxLimiterError>;
}
```

**Implementations**:

#### Token Bucket

```rust
pub struct TokenBucketAlgorithm {
    rate: f64,
    capacity: f64,
    client_state: DashMap<String, TokenBucketState>,
}

struct TokenBucketState {
    tokens: f64,
    last_refill: u64,
}
```

#### Sliding Window

```rust
pub struct SlidingWindowAlgorithm {
    window_size_nanos: u64,
    max_requests: u64,
    client_state: DashMap<String, VecDeque<u64>>,
}
```

#### Leaky Bucket

```rust
pub struct LeakyBucketAlgorithm {
    rate: f64,
    capacity: u64,
    client_state: DashMap<String, LeakyBucketState>,
}
```

**Benefits**:
- Algorithm flexibility
- Use case specific optimization
- Easy comparison

**Implementation Strategy**:
- Feature flag: `algorithms`
- Separate modules
- Common trait interface

### 5. Distributed Rate Limiting

Coordinate rate limits across multiple instances.

```rust
pub struct DistributedFluxLimiter<T, C, S>
where
    T: Hash + Eq + Clone + Send + Sync,
    C: Clock,
    S: StateStore,
{
    local: FluxLimiter<T, C>,
    store: S,
    sync_interval: Duration,
}

impl<T, C, S> DistributedFluxLimiter<T, C, S> {
    pub async fn check_request(
        &self,
        client_id: T,
    ) -> Result<FluxLimiterDecision, FluxLimiterError> {
        // 1. Check local cache
        // 2. If miss, fetch from store
        // 3. Make decision
        // 4. Update both local and remote
    }

    async fn sync_state(&self) -> Result<(), StoreError> {
        // Periodic sync with remote store
    }
}
```

**Strategies**:
- **Best-effort**: Local-first with periodic sync
- **Strict**: Remote check every time (slower but accurate)
- **Hybrid**: Local fast path, remote for edge cases

**Benefits**:
- Multi-instance coordination
- Cluster-wide rate limits
- Horizontal scaling

### 6. Adaptive Rate Limiting

Dynamically adjust rates based on backend health.

```rust
pub struct AdaptiveRateLimiter<T, C, H>
where
    T: Hash + Eq + Clone,
    C: Clock,
    H: HealthCheck,
{
    base_config: FluxLimiterConfig,
    limiter: RwLock<FluxLimiter<T, C>>,
    health_check: H,
    adjustment_factor: AtomicU64, // Fixed-point percentage
}

impl<T, C, H> AdaptiveRateLimiter<T, C, H> {
    pub async fn adjust_rate(&self) -> Result<(), FluxLimiterError> {
        let health = self.health_check.check().await;

        let new_factor = match health {
            Health::Good => 1.0,      // Normal rate
            Health::Degraded => 0.7,  // 70% of normal
            Health::Bad => 0.3,       // 30% of normal
        };

        // Update limiter with adjusted rate
        let new_rate = self.base_config.rate() * new_factor;
        // ... recreate limiter ...
    }
}
```

**Benefits**:
- Protect degraded backends
- Auto-recovery
- Dynamic adjustment

### 7. Quota Management

Track cumulative usage over longer periods.

```rust
pub struct QuotaManager<T>
where
    T: Hash + Eq + Clone,
{
    limiter: FluxLimiter<T, SystemClock>,
    quotas: DashMap<T, QuotaState>,
}

struct QuotaState {
    total_allowed: u64,
    period_start: u64,
    quota_limit: u64,
}

impl<T> QuotaManager<T> {
    pub fn check_request(
        &self,
        client_id: T,
    ) -> Result<QuotaDecision, FluxLimiterError> {
        // 1. Check rate limit
        // 2. Check quota
        // 3. Update quota usage
        // 4. Return combined decision
    }
}
```

**Benefits**:
- Long-term limits (daily, monthly)
- Billing integration
- Multi-tier quotas

## Extensibility Patterns

### Plugin Architecture

```rust
pub trait FluxLimiterPlugin: Send + Sync {
    fn before_check(&self, client_id: &str);
    fn after_check(&self, client_id: &str, decision: &FluxLimiterDecision);
    fn on_error(&self, client_id: &str, error: &FluxLimiterError);
}

pub struct PluggableFluxLimiter<T, C>
where
    T: Hash + Eq + Clone,
    C: Clock,
{
    limiter: FluxLimiter<T, C>,
    plugins: Vec<Box<dyn FluxLimiterPlugin>>,
}
```

### Middleware Pattern

```rust
pub trait RateLimitMiddleware {
    fn wrap(
        &self,
        inner: Box<dyn Fn(&str) -> Result<FluxLimiterDecision, FluxLimiterError>>,
    ) -> Box<dyn Fn(&str) -> Result<FluxLimiterDecision, FluxLimiterError>>;
}

// Example: Logging middleware
pub struct LoggingMiddleware;

impl RateLimitMiddleware for LoggingMiddleware {
    fn wrap(&self, inner: Box<dyn Fn(&str) -> ...>) -> Box<dyn Fn(&str) -> ...> {
        Box::new(move |client_id| {
            info!("Checking rate limit for {}", client_id);
            let result = inner(client_id);
            info!("Result: {:?}", result);
            result
        })
    }
}
```

## Backward Compatibility Strategy

### Semantic Versioning

- **Patch (0.4.x)**: Bug fixes, performance improvements
- **Minor (0.x.0)**: New features, backward compatible
- **Major (x.0.0)**: Breaking changes

### Feature Flags

All new features behind feature flags:

```toml
[features]
default = []
async = ["tokio"]
redis = ["redis", "async"]
prometheus = ["prometheus"]
opentelemetry = ["opentelemetry"]
algorithms = []
```

### Deprecation Path

1. **Deprecate**: Mark old API with `#[deprecated]`
2. **Transition Period**: Support both old and new (2-3 minor versions)
3. **Remove**: Remove in next major version

### Type Aliases

Maintain existing API surface:

```rust
// Existing users can continue using this
pub type FluxLimiter<T> = FluxLimiter<T, SystemClock>;

// New users can use explicit clock type
pub type FluxLimiter<T, C> = core::FluxLimiter<T, C>;
```

## Implementation Roadmap

### Phase 1: Core Improvements (v0.5)
- [ ] Async support
- [ ] Enhanced metrics
- [ ] Performance optimizations

### Phase 2: Persistence (v0.6)
- [ ] State store trait
- [ ] Redis backend
- [ ] PostgreSQL backend

### Phase 3: Advanced Features (v0.7)
- [ ] Alternative algorithms
- [ ] Distributed coordination
- [ ] Adaptive rate limiting

### Phase 4: Enterprise Features (v1.0)
- [ ] Quota management
- [ ] Multi-tier support
- [ ] Advanced monitoring

## Contributing

We welcome contributions! Areas of interest:

1. **Performance**: Benchmarks and optimizations
2. **Backends**: New state store implementations
3. **Metrics**: Monitoring integrations
4. **Algorithms**: Alternative rate limiting strategies
5. **Documentation**: Examples and guides
6. **Testing**: More comprehensive tests

## Next Steps

- [Overview](./overview.md) - Understand current architecture
- [Design Decisions](./design-decisions.md) - Why we made these choices
- [Contributing](../../README.md) - How to contribute