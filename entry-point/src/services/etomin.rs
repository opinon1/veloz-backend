//! Etomin payment-gateway HTTP client.
//!
//! Two endpoints used:
//!   POST /api/v1/signin  → returns a JWT (treated as opaque, cached in Redis)
//!   POST /api/v1/sale    → charges a card; returns APPROVED | DECLINED | PENDING
//!
//! Notes / known limitations:
//!   - Path C: raw `cardNumber` + `cvv` are sent in the /sale body. PCI scope
//!     is the operator's responsibility (this codebase is being sold to the
//!     processor's owner per project context). Don't log card data.
//!   - PENDING (3DS) returns a `redirectTo` URL. Reconciliation after the
//!     user completes 3DS is not handled here — Etomin's webhook / status
//!     query endpoints aren't in the public docs at time of writing.
//!   - JWT cached in Redis under `etomin:jwt` with TTL = 23h. On a 401 from
//!     /sale we evict and re-signin once before giving up.
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use std::env;

const JWT_REDIS_KEY: &str = "etomin:jwt";
const JWT_TTL_SECS: i64 = 23 * 60 * 60;

#[derive(Debug)]
pub enum EtominError {
    /// Etomin's signin rejected our credentials.
    Auth,
    /// Etomin returned a non-2xx for a /sale call (after one re-signin retry).
    Upstream(String),
    /// Local infra failure (DNS, Redis, JSON parse).
    Local(String),
}

impl std::fmt::Display for EtominError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EtominError::Auth => write!(f, "etomin auth failed"),
            EtominError::Upstream(s) => write!(f, "etomin upstream: {s}"),
            EtominError::Local(s) => write!(f, "etomin local: {s}"),
        }
    }
}

#[derive(Serialize)]
struct SigninRequest<'a> {
    email: &'a str,
    password: &'a str,
}

#[derive(Deserialize)]
struct SigninResponse {
    /// Etomin sandbox returns `authToken`. Fall back to `token` / `data.token`
    /// for safety against tenant variation.
    #[serde(rename = "authToken")]
    auth_token: Option<String>,
    token: Option<String>,
    data: Option<SigninResponseData>,
}

#[derive(Deserialize)]
struct SigninResponseData {
    token: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct SaleRequest<'a> {
    pub amount: i64,
    pub currency: &'a str,
    pub reference: &'a str,
    #[serde(rename = "customerInformation")]
    pub customer_information: serde_json::Value,
    #[serde(rename = "cardData")]
    pub card_data: serde_json::Value,
    #[serde(rename = "redirectUrl", skip_serializing_if = "Option::is_none")]
    pub redirect_url: Option<&'a str>,
}

#[derive(Clone)]
pub struct EtominClient {
    base_url: String,
    email: String,
    password: String,
    redis: redis::aio::ConnectionManager,
    http: reqwest::Client,
}

impl EtominClient {
    pub fn from_env(redis: redis::aio::ConnectionManager) -> Result<Self, EtominError> {
        let base_url = env::var("ETOMIN_BASE_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "https://pagos.etomin.com".to_string());
        // Treat unset OR empty-string as "not configured" so leaving the
        // entry blank in .env disables the client cleanly.
        let email = env::var("ETOMIN_EMAIL")
            .ok()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| EtominError::Local("ETOMIN_EMAIL missing".into()))?;
        let password = env::var("ETOMIN_PASSWORD")
            .ok()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| EtominError::Local("ETOMIN_PASSWORD missing".into()))?;
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| EtominError::Local(e.to_string()))?;
        Ok(Self {
            base_url,
            email,
            password,
            redis,
            http,
        })
    }

    /// Get a JWT, using the Redis cache if present. On cache miss, sign in.
    async fn token(&self) -> Result<String, EtominError> {
        let mut redis = self.redis.clone();
        if let Ok(Some(t)) = redis.get::<_, Option<String>>(JWT_REDIS_KEY).await {
            return Ok(t);
        }
        self.refresh_token().await
    }

    async fn refresh_token(&self) -> Result<String, EtominError> {
        let url = format!("{}/api/v1/signin", self.base_url);
        let body = SigninRequest {
            email: &self.email,
            password: &self.password,
        };
        let res = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| EtominError::Local(e.to_string()))?;
        if !res.status().is_success() {
            return Err(EtominError::Auth);
        }
        let parsed: SigninResponse = res
            .json()
            .await
            .map_err(|e| EtominError::Local(format!("signin parse: {e}")))?;
        let token = parsed
            .auth_token
            .or(parsed.token)
            .or_else(|| parsed.data.and_then(|d| d.token))
            .ok_or_else(|| EtominError::Local("signin: no token in response".into()))?;

        let mut redis = self.redis.clone();
        let _: redis::RedisResult<()> = redis
            .set_ex(JWT_REDIS_KEY, token.clone(), JWT_TTL_SECS as u64)
            .await;
        Ok(token)
    }

    /// POST /api/v1/sale. On 401 we evict the cached JWT, re-signin once, and
    /// retry the call. Returns the raw JSON Etomin responded with so the
    /// caller can persist it for audit.
    pub async fn sale(&self, payload: &SaleRequest<'_>) -> Result<serde_json::Value, EtominError> {
        let token = self.token().await?;
        match self.try_sale(&token, payload).await {
            Ok(v) => Ok(v),
            Err(EtominError::Auth) => {
                // Token may have expired before the cache TTL elapsed (Etomin
                // sometimes invalidates server-side). Refresh once and retry.
                let mut redis = self.redis.clone();
                let _: redis::RedisResult<i64> = redis.del(JWT_REDIS_KEY).await;
                let token = self.refresh_token().await?;
                self.try_sale(&token, payload).await
            }
            Err(e) => Err(e),
        }
    }

    async fn try_sale(
        &self,
        token: &str,
        payload: &SaleRequest<'_>,
    ) -> Result<serde_json::Value, EtominError> {
        let url = format!("{}/api/v1/sale", self.base_url);
        let res = self
            .http
            .post(&url)
            .bearer_auth(token)
            .json(payload)
            .send()
            .await
            .map_err(|e| EtominError::Local(e.to_string()))?;
        let status = res.status();
        let body: serde_json::Value = res
            .json()
            .await
            .unwrap_or(serde_json::json!({"_unparseable": true}));
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(EtominError::Auth);
        }
        if !status.is_success() {
            return Err(EtominError::Upstream(format!("HTTP {status}: {body}")));
        }
        Ok(body)
    }
}
