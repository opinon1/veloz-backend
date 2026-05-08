use axum::{extract::State, Json, http::StatusCode};
use rand::Rng;
use redis::AsyncCommands;
use serde::Serialize;
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::Claims;
use crate::handlers::grants_util::apply_grant;
use crate::models::store_types::validate_grants;
use super::{cooldown_key, COOLDOWN_SECS};

#[derive(Serialize)]
pub struct SpinResponse {
    /// Index into the wheel's `items` array (sorted by `position`) so the UI
    /// animates landing on the right slice.
    pub won_index: usize,
    /// Snapshot of the won reward (Grant array).
    pub reward: serde_json::Value,
    /// Updated wallet balances post-fulfillment.
    pub new_balances: Balances,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Balances {
    pub high: i64,
    pub soft: i64,
    pub energy: i64,
}

#[derive(sqlx::FromRow)]
struct ItemRow {
    id: Uuid,
    reward: serde_json::Value,
    weight: i32,
}

/// POST /prize-wheel/spin — atomic 24h cooldown via Redis SET NX EX, weighted
/// pick, transactional reward fulfillment + spin history insert.
///
///   503 — wheel has no items configured
///   429 — cooldown still active (returns retry_after_seconds in body)
///   200 — won_index + reward + new wallet balances
pub async fn spin(
    State(state): State<AppState>,
    Claims(session): Claims,
) -> Result<Json<SpinResponse>, (StatusCode, Json<serde_json::Value>)> {
    let mut redis = state.redis.clone();

    // SET key value NX EX 86400 — only sets if missing. Returns OK on first
    // win, nil on cooldown active. Atomic, so two concurrent spins can't
    // both succeed.
    let acquired: Option<String> = redis::cmd("SET")
        .arg(cooldown_key(session.user_id))
        .arg(chrono::Utc::now().timestamp())
        .arg("NX")
        .arg("EX")
        .arg(COOLDOWN_SECS)
        .query_async(&mut redis)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({}))))?;

    if acquired.is_none() {
        let ttl: i64 = redis
            .ttl(cooldown_key(session.user_id))
            .await
            .unwrap_or(0);
        let retry = if ttl < 0 { 0 } else { ttl };
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "error": "cooldown",
                "retry_after_seconds": retry,
            })),
        ));
    }

    let items = sqlx::query_as::<_, ItemRow>(
        "SELECT id, reward, weight FROM prize_wheel_items ORDER BY position ASC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({}))))?;

    if items.is_empty() {
        // Roll back the cooldown so the user isn't punished for an unconfigured wheel.
        let _: redis::RedisResult<i64> = redis.del(cooldown_key(session.user_id)).await;
        return Err((StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({
            "error": "wheel_empty",
        }))));
    }

    // Weighted pick.
    let total_weight: i64 = items.iter().map(|i| i.weight as i64).sum();
    let mut roll = rand::thread_rng().gen_range(0..total_weight);
    let mut won_index = 0usize;
    let mut won_id = items[0].id;
    let mut won_reward = items[0].reward.clone();
    for (idx, item) in items.iter().enumerate() {
        if roll < item.weight as i64 {
            won_index = idx;
            won_id = item.id;
            won_reward = item.reward.clone();
            break;
        }
        roll -= item.weight as i64;
    }

    let grants = validate_grants(&won_reward).map_err(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({})))
    })?;

    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({}))))?;

    // Record the spin (snapshots the reward) and use its id as the audit
    // reference for any wallet-ledger entries the grants produce.
    let (spin_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO prize_wheel_spins (user_id, reward) VALUES ($1, $2) RETURNING id",
    )
    .bind(session.user_id)
    .bind(&won_reward)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({}))))?;

    for g in &grants {
        apply_grant(&mut tx, session.user_id, g, "prize_wheel", spin_id)
            .await
            .map_err(|s| (s, Json(serde_json::json!({}))))?;
    }

    let balances: Balances = sqlx::query_as(
        "SELECT high, soft, energy FROM wallets WHERE user_id = $1",
    )
    .bind(session.user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({}))))?;

    tx.commit()
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({}))))?;

    let _ = won_id; // referenced via ItemRow id; keep variable to make intent clear

    Ok(Json(SpinResponse {
        won_index,
        reward: won_reward,
        new_balances: balances,
    }))
}
