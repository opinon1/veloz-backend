use axum::{extract::State, Json, http::StatusCode};
use redis::AsyncCommands;
use serde::Serialize;
use crate::state::AppState;
use crate::extractors::Claims;
use super::cooldown_key;

#[derive(Serialize)]
pub struct CooldownResponse {
    pub ready: bool,
    pub retry_after_seconds: i64,
}

/// GET /prize-wheel/cooldown — TTL of the Redis key, or 0 if no key set.
pub async fn get_cooldown(
    State(state): State<AppState>,
    Claims(session): Claims,
) -> Result<Json<CooldownResponse>, StatusCode> {
    let mut redis = state.redis.clone();
    let ttl: i64 = redis
        .ttl(cooldown_key(session.user_id))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Redis TTL semantics: -2 = key missing, -1 = key without TTL, ≥0 = secs.
    // Either of the negative cases means "no cooldown active".
    let retry_after_seconds = if ttl < 0 { 0 } else { ttl };
    Ok(Json(CooldownResponse {
        ready: retry_after_seconds == 0,
        retry_after_seconds,
    }))
}
