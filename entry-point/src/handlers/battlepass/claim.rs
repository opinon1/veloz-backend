use axum::{extract::{Path, State}, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::Claims;
use super::utils::active_season;

#[derive(Deserialize)]
pub struct ClaimRequest {
    pub track: String,     // 'free' | 'premium'
}

#[derive(Serialize)]
pub struct ClaimResponse {
    pub tier: i32,
    pub track: String,
    pub reward: serde_json::Value,
}

/// Grants the reward for a given tier on a given track. The reward payload is
/// opaque JSON — actual fulfillment (granting currency, unlocking skin) is the
/// frontend's + game server's responsibility using the payload structure.
/// Server records the claim so the reward can't be taken twice.
pub async fn claim_tier(
    State(state): State<AppState>,
    Claims(session): Claims,
    Path(tier): Path<i32>,
    Json(payload): Json<ClaimRequest>,
) -> Result<Json<ClaimResponse>, StatusCode> {
    if payload.track != "free" && payload.track != "premium" {
        return Err(StatusCode::BAD_REQUEST);
    }

    let season = active_season(&state.db).await?.ok_or(StatusCode::NOT_FOUND)?;

    let mut tx = state.db.begin().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let tier_row: Option<(Uuid, i64, serde_json::Value, serde_json::Value)> = sqlx::query_as(
        "SELECT id, xp_required, free_reward, premium_reward FROM bp_tiers WHERE season_id = $1 AND tier = $2",
    )
    .bind(season.id)
    .bind(tier)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (_tier_id, xp_required, free_reward, premium_reward) =
        tier_row.ok_or(StatusCode::NOT_FOUND)?;

    let progress: Option<(i64, bool)> = sqlx::query_as(
        "SELECT bp_xp, premium_unlocked FROM bp_progress WHERE user_id = $1 AND season_id = $2",
    )
    .bind(session.user_id)
    .bind(season.id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (bp_xp, premium_unlocked) = progress.unwrap_or((0, false));

    if bp_xp < xp_required {
        return Err(StatusCode::FORBIDDEN);
    }
    if payload.track == "premium" && !premium_unlocked {
        return Err(StatusCode::PAYMENT_REQUIRED);
    }

    let reward = if payload.track == "free" { free_reward } else { premium_reward };

    let insert = sqlx::query(
        r#"INSERT INTO bp_claims (user_id, season_id, tier, track)
           VALUES ($1, $2, $3, $4)
           ON CONFLICT DO NOTHING"#,
    )
    .bind(session.user_id)
    .bind(season.id)
    .bind(tier)
    .bind(&payload.track)
    .execute(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if insert.rows_affected() == 0 {
        return Err(StatusCode::CONFLICT); // already claimed
    }

    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(ClaimResponse {
        tier,
        track: payload.track,
        reward,
    }))
}
