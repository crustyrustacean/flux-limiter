# Web Framework Integration

Learn how to integrate Flux Limiter with popular web frameworks.

## Axum Integration

### Basic Middleware

```rust
use axum::{
    extract::State,
    http::{Request, StatusCode, HeaderMap},
    middleware::Next,
    response::{Response, IntoResponse},
};
use flux_limiter::{FluxLimiter, FluxLimiterError, SystemClock};
use std::sync::Arc;

type SharedLimiter = Arc<FluxLimiter<String, SystemClock>>;

async fn rate_limit_middleware<B>(
    State(limiter): State<SharedLimiter>,
    request: Request<B>,
    next: Next<B>,
) -> Result<Response, (StatusCode, HeaderMap, &'static str)> {
    // Extract client ID (e.g., from IP or auth header)
    let client_ip = extract_client_ip(&request);

    match limiter.check_request(client_ip.clone()) {
        Ok(decision) if decision.allowed => {
            // Add rate limit headers
            let mut response = next.run(request).await;
            if let Some(remaining) = decision.remaining_capacity {
                response.headers_mut().insert(
                    "X-RateLimit-Remaining",
                    remaining.to_string().parse().unwrap(),
                );
            }
            Ok(response)
        }
        Ok(decision) => {
            // Rate limited
            let mut headers = HeaderMap::new();
            if let Some(retry_after) = decision.retry_after_seconds {
                headers.insert(
                    "Retry-After",
                    (retry_after.ceil() as u64).to_string().parse().unwrap(),
                );
            }
            headers.insert("X-RateLimit-Remaining", "0".parse().unwrap());

            Err((StatusCode::TOO_MANY_REQUESTS, headers, "Rate limited"))
        }
        Err(FluxLimiterError::ClockError(_)) => {
            // Fail-open policy
            eprintln!("Rate limiter clock error - allowing request");
            Ok(next.run(request).await)
        }
        Err(e) => {
            eprintln!("Rate limiter error: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, HeaderMap::new(), "Internal error"))
        }
    }
}

fn extract_client_ip<B>(request: &Request<B>) -> String {
    request
        .headers()
        .get("X-Forwarded-For")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.split(',').next())
        .unwrap_or("unknown")
        .to_string()
}
```

### Complete Axum Example

```rust
use axum::{
    routing::get,
    Router,
    middleware,
};
use flux_limiter::{FluxLimiter, FluxLimiterConfig, SystemClock};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    // Create rate limiter
    let config = FluxLimiterConfig::new(10.0, 5.0);
    let limiter = Arc::new(FluxLimiter::with_config(config, SystemClock).unwrap());

    // Build router with middleware
    let app = Router::new()
        .route("/api/data", get(handler))
        .layer(middleware::from_fn_with_state(
            limiter.clone(),
            rate_limit_middleware,
        ))
        .with_state(limiter);

    // Run server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();
}

async fn handler() -> &'static str {
    "Hello, world!"
}
```

## Actix-Web Integration

### Basic Middleware

```rust
use actix_web::{
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
    Error, HttpResponse, http::header,
};
use flux_limiter::{FluxLimiter, FluxLimiterError, SystemClock};
use std::sync::Arc;
use std::future::Future;
use std::pin::Pin;

pub struct RateLimitMiddleware {
    limiter: Arc<FluxLimiter<String, SystemClock>>,
}

impl RateLimitMiddleware {
    pub fn new(limiter: Arc<FluxLimiter<String, SystemClock>>) -> Self {
        Self { limiter }
    }
}

impl<S, B> Transform<S, ServiceRequest> for RateLimitMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Transform = RateLimitMiddlewareService<S>;
    type InitError = ();
    type Future = Pin<Box<dyn Future<Output = Result<Self::Transform, Self::InitError>>>>;

    fn new_transform(&self, service: S) -> Self::Future {
        let limiter = self.limiter.clone();
        Box::pin(async move {
            Ok(RateLimitMiddlewareService { service, limiter })
        })
    }
}

pub struct RateLimitMiddlewareService<S> {
    service: S,
    limiter: Arc<FluxLimiter<String, SystemClock>>,
}

impl<S, B> Service<ServiceRequest> for RateLimitMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let client_ip = req
            .connection_info()
            .realip_remote_addr()
            .unwrap_or("unknown")
            .to_string();

        let decision = self.limiter.check_request(client_ip);

        match decision {
            Ok(decision) if decision.allowed => {
                let fut = self.service.call(req);
                Box::pin(async move {
                    let mut res = fut.await?;
                    if let Some(remaining) = decision.remaining_capacity {
                        res.headers_mut().insert(
                            header::HeaderName::from_static("x-ratelimit-remaining"),
                            header::HeaderValue::from_str(&remaining.to_string()).unwrap(),
                        );
                    }
                    Ok(res)
                })
            }
            Ok(decision) => {
                Box::pin(async move {
                    let mut response = HttpResponse::TooManyRequests();
                    if let Some(retry_after) = decision.retry_after_seconds {
                        response.insert_header((
                            "Retry-After",
                            (retry_after.ceil() as u64).to_string(),
                        ));
                    }
                    Ok(req.into_response(response.finish()))
                })
            }
            Err(FluxLimiterError::ClockError(_)) => {
                // Fail-open policy
                eprintln!("Clock error - allowing request");
                let fut = self.service.call(req);
                Box::pin(async move { fut.await })
            }
            Err(e) => {
                eprintln!("Rate limiter error: {}", e);
                Box::pin(async move {
                    Ok(req.into_response(HttpResponse::InternalServerError().finish()))
                })
            }
        }
    }
}
```

## Rocket Integration

### Request Guard

```rust
use rocket::{
    request::{FromRequest, Outcome, Request},
    http::Status,
    State,
};
use flux_limiter::{FluxLimiter, FluxLimiterDecision, SystemClock};
use std::sync::Arc;

pub struct RateLimited {
    pub decision: FluxLimiterDecision,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for RateLimited {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let limiter = request
            .guard::<&State<Arc<FluxLimiter<String, SystemClock>>>>()
            .await
            .unwrap();

        let client_ip = request
            .client_ip()
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| "unknown".to_string());

        match limiter.check_request(client_ip) {
            Ok(decision) if decision.allowed => {
                Outcome::Success(RateLimited { decision })
            }
            Ok(_) => Outcome::Error((Status::TooManyRequests, ())),
            Err(_) => {
                // Fail-open
                Outcome::Success(RateLimited {
                    decision: FluxLimiterDecision {
                        allowed: true,
                        retry_after_seconds: None,
                        remaining_capacity: None,
                        reset_time_nanos: 0,
                    },
                })
            }
        }
    }
}

// Usage in route
#[rocket::get("/api/data")]
fn data(_rate_limited: RateLimited) -> &'static str {
    "Hello, world!"
}
```

## Standard Rate Limit Headers

### HTTP Response Headers

Implement standard rate limiting headers:

```rust
use axum::http::HeaderMap;

fn add_rate_limit_headers(
    headers: &mut HeaderMap,
    decision: &FluxLimiterDecision,
    limit: f64,
) {
    // X-RateLimit-Limit: Maximum requests allowed
    headers.insert(
        "X-RateLimit-Limit",
        limit.to_string().parse().unwrap(),
    );

    // X-RateLimit-Remaining: Remaining requests in window
    if let Some(remaining) = decision.remaining_capacity {
        headers.insert(
            "X-RateLimit-Remaining",
            remaining.to_string().parse().unwrap(),
        );
    }

    // X-RateLimit-Reset: When the limit resets (Unix timestamp)
    let reset_seconds = decision.reset_time_nanos / 1_000_000_000;
    headers.insert(
        "X-RateLimit-Reset",
        reset_seconds.to_string().parse().unwrap(),
    );

    // Retry-After: When to retry (for 429 responses)
    if !decision.allowed {
        if let Some(retry_after) = decision.retry_after_seconds {
            headers.insert(
                "Retry-After",
                (retry_after.ceil() as u64).to_string().parse().unwrap(),
            );
        }
    }
}
```

## Multi-Tier Rate Limiting

Rate limit based on user tier:

```rust
use axum::{
    extract::{State, Extension},
    http::Request,
    middleware::Next,
    response::Response,
};
use std::collections::HashMap;
use std::sync::Arc;

struct TieredLimiters {
    limiters: HashMap<String, Arc<FluxLimiter<String, SystemClock>>>,
}

impl TieredLimiters {
    fn new() -> Result<Self, FluxLimiterError> {
        let mut limiters = HashMap::new();

        limiters.insert(
            "free".to_string(),
            Arc::new(FluxLimiter::with_config(
                FluxLimiterConfig::new(10.0, 5.0),
                SystemClock
            )?),
        );

        limiters.insert(
            "paid".to_string(),
            Arc::new(FluxLimiter::with_config(
                FluxLimiterConfig::new(100.0, 50.0),
                SystemClock
            )?),
        );

        Ok(Self { limiters })
    }

    fn get_limiter(&self, tier: &str) -> &Arc<FluxLimiter<String, SystemClock>> {
        self.limiters.get(tier).unwrap_or_else(|| &self.limiters["free"])
    }
}

async fn tiered_rate_limit<B>(
    State(limiters): State<Arc<TieredLimiters>>,
    Extension(user_tier): Extension<String>,
    request: Request<B>,
    next: Next<B>,
) -> Response {
    let client_ip = extract_client_ip(&request);
    let limiter = limiters.get_limiter(&user_tier);

    match limiter.check_request(client_ip) {
        Ok(decision) if decision.allowed => next.run(request).await,
        Ok(_) => {
            (StatusCode::TOO_MANY_REQUESTS, "Rate limited").into_response()
        }
        Err(_) => next.run(request).await, // Fail-open
    }
}
```

## Background Cleanup Task

Automatically clean up stale clients:

```rust
use tokio::time::{interval, Duration};
use std::sync::Arc;

async fn cleanup_task(limiter: Arc<FluxLimiter<String, SystemClock>>) {
    let mut cleanup_interval = interval(Duration::from_secs(3600)); // 1 hour

    loop {
        cleanup_interval.tick().await;

        let threshold = 24 * 60 * 60 * 1_000_000_000u64; // 24 hours

        match limiter.cleanup_stale_clients(threshold) {
            Ok(()) => {
                // Cleanup succeeded
            }
            Err(e) => {
                eprintln!("Rate limiter cleanup failed: {}", e);
            }
        }
    }
}

// Start cleanup task when initializing the server
#[tokio::main]
async fn main() {
    let config = FluxLimiterConfig::new(100.0, 50.0);
    let limiter = Arc::new(FluxLimiter::with_config(config, SystemClock).unwrap());

    // Start cleanup in background
    tokio::spawn(cleanup_task(limiter.clone()));

    // Build and run your web server...
}
```

## Next Steps

- [Production Considerations](./production.md) - Deploy with confidence
- [Error Handling](./error-handling.md) - Handle errors gracefully
- [Advanced Usage](./advanced-usage.md) - Optimize performance