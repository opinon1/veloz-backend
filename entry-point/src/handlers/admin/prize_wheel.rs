use axum::{extract::State, Json, http::StatusCode};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::AdminClaims;
use crate::handlers::prize_wheel::cooldown_key;
use crate::models::store_types::validate_grants;

#[derive(Deserialize)]
pub struct WheelItemInput {
    pub reward: serde_json::Value,
    pub weight: i32,
}

#[derive(Deserialize)]
pub struct PutWheelRequest {
    pub items: Vec<WheelItemInput>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct WheelItemRow {
    pub id: Uuid,
    pub position: i32,
    pub reward: serde_json::Value,
    pub weight: i32,
}

/// PUT /admin/prize-wheel — replace the entire wheel atomically.
pub async fn put_wheel(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Json(payload): Json<PutWheelRequest>,
) -> Result<Json<Vec<WheelItemRow>>, StatusCode> {
    if payload.items.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    for item in &payload.items {
        if item.weight < 1 {
            return Err(StatusCode::BAD_REQUEST);
        }
        validate_grants(&item.reward).map_err(|_| StatusCode::BAD_REQUEST)?;
    }

    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query("DELETE FROM prize_wheel_items")
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    for (idx, item) in payload.items.iter().enumerate() {
        sqlx::query(
            "INSERT INTO prize_wheel_items (position, reward, weight) VALUES ($1, $2, $3)",
        )
        .bind(idx as i32)
        .bind(&item.reward)
        .bind(item.weight)
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rows = sqlx::query_as::<_, WheelItemRow>(
        "SELECT id, position, reward, weight FROM prize_wheel_items ORDER BY position ASC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(rows))
}

/// GET /admin/prize-wheel — inspect the current wheel.
pub async fn get_wheel(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
) -> Result<Json<Vec<WheelItemRow>>, StatusCode> {
    let rows = sqlx::query_as::<_, WheelItemRow>(
        "SELECT id, position, reward, weight FROM prize_wheel_items ORDER BY position ASC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(rows))
}

/// DELETE /admin/prize-wheel/cooldown — clear THIS admin's cooldown so they
/// can spin again. Intentionally only their own; doesn't touch other users.
pub async fn delete_self_cooldown(
    State(state): State<AppState>,
    AdminClaims(session): AdminClaims,
) -> Result<StatusCode, StatusCode> {
    let mut redis = state.redis.clone();
    let _: redis::RedisResult<i64> = redis.del(cooldown_key(session.user_id)).await;
    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /admin/prize-wheel — empty the wheel.
///
/// PUT requires a non-empty items array, so without this there's no way to
/// reach the "no items configured" state. Useful both as an ops escape
/// hatch and to drive the 503 path in tests.
pub async fn delete_wheel(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
) -> Result<StatusCode, StatusCode> {
    sqlx::query("DELETE FROM prize_wheel_items")
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}
