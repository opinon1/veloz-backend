use axum::{extract::State, Json, http::StatusCode};
use redis::AsyncCommands;
use serde::Serialize;
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::Claims;
use super::{cooldown_key, cooldown::CooldownResponse};

#[derive(Serialize, sqlx::FromRow)]
pub struct WheelItem {
    pub id: Uuid,
    pub position: i32,
    pub reward: serde_json::Value,
    pub weight: i32,
}

#[derive(Serialize)]
pub struct WheelResponse {
    pub items: Vec<WheelItem>,
    pub cooldown: CooldownResponse,
}

/// GET /prize-wheel — current wheel + caller's cooldown state.
pub async fn get_wheel(
    State(state): State<AppState>,
    Claims(session): Claims,
) -> Result<Json<WheelResponse>, StatusCode> {
    let items = sqlx::query_as::<_, WheelItem>(
        "SELECT id, position, reward, weight FROM prize_wheel_items ORDER BY position ASC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut redis = state.redis.clone();
    let ttl: i64 = redis
        .ttl(cooldown_key(session.user_id))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let retry_after_seconds = if ttl < 0 { 0 } else { ttl };

    Ok(Json(WheelResponse {
        items,
        cooldown: CooldownResponse {
            ready: retry_after_seconds == 0,
            retry_after_seconds,
        },
    }))
}
