use axum::{
    body::Body,
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
};
use redis::AsyncCommands;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tower::{Layer, Service, ServiceExt};

/// Redis-backed fixed-window rate limiter.
///
/// Keys in Redis: `rate_limit:{key_prefix}:{client_ip}`
///
/// # Example
/// ```
/// .route("/signin", post(signin).layer(
///     RateLimitLayer::new(redis.clone(), 10, 60, "signin")
/// ))
/// ```
#[derive(Clone)]
pub struct RateLimitLayer {
    redis: redis::aio::ConnectionManager,
    max_requests: i64,
    window_secs: i64,
    key_prefix: String,
}

impl RateLimitLayer {
    /// - `max_requests`: maximum requests allowed per window
    /// - `window_secs`: window duration in seconds
    /// - `key_prefix`: namespaces the Redis key (e.g. `"signin"`, `"signup"`)
    pub fn new(
        redis: redis::aio::ConnectionManager,
        max_requests: i64,
        window_secs: i64,
        key_prefix: impl Into<String>,
    ) -> Self {
        Self {
            redis,
            max_requests,
            window_secs,
            key_prefix: key_prefix.into(),
        }
    }
}

impl<S> Layer<S> for RateLimitLayer {
    type Service = RateLimitService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RateLimitService {
            inner,
            redis: self.redis.clone(),
            max_requests: self.max_requests,
            window_secs: self.window_secs,
            key_prefix: self.key_prefix.clone(),
        }
    }
}

#[derive(Clone)]
pub struct RateLimitService<S> {
    inner: S,
    redis: redis::aio::ConnectionManager,
    max_requests: i64,
    window_secs: i64,
    key_prefix: String,
}

impl<S> Service<Request<Body>> for RateLimitService<S>
where
    S: Service<Request<Body>, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        // X-Forwarded-For is set by reverse proxies (Dokploy/nginx in production).
        // Fall back to the real peer address for direct connections (localhost).
        let ip = req
            .headers()
            .get("X-Forwarded-For")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next())
            .map(|s| s.trim().to_string())
            .or_else(|| {
                req.extensions()
                    .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
                    .map(|info| info.0.ip().to_string())
            })
            .unwrap_or_else(|| "unknown".to_string());

        let mut redis = self.redis.clone();
        let max_requests = self.max_requests;
        let window_secs = self.window_secs;
        let key = format!("rate_limit:{}:{}", self.key_prefix, ip);

        // Take the already-polled-ready inner service; replace self.inner with a
        // fresh clone for the next request. This upholds the Tower poll_ready contract.
        let fresh = self.inner.clone();
        let inner = std::mem::replace(&mut self.inner, fresh);

        Box::pin(async move {
            let count: i64 = match redis.incr::<_, _, i64>(&key, 1i64).await {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!(key = %key, error = %e, "rate limiter Redis error — failing open");
                    0
                }
            };

            // Set TTL only on the first request so the window starts fresh
            if count == 1 {
                if let Err(e) = redis.expire::<_, ()>(&key, window_secs).await {
                    tracing::error!(key = %key, error = %e, "rate limiter failed to set TTL");
                }
            }

            if count > max_requests {
                tracing::warn!(key = %key, count, max_requests, "rate limit exceeded");
                return Ok(StatusCode::TOO_MANY_REQUESTS.into_response());
            }

            // Use oneshot so poll_ready is called on the inner service before call
            inner.oneshot(req).await
        })
    }
}
