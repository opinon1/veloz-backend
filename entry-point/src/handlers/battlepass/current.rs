use axum::{extract::State, Json, http::StatusCode};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;
use crate::state::AppState;
use super::utils::active_season;

#[derive(Serialize, sqlx::FromRow)]
pub struct TierRow {
    pub id: Uuid,
    pub tier: i32,
    pub xp_required: i64,
    pub free_reward: serde_json::Value,
    pub premium_reward: serde_json::Value,
}

#[derive(Serialize)]
pub struct CurrentSeasonResponse {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub premium_cost: i64,
    pub premium_currency: String,
    pub tiers: Vec<TierRow>,
}

pub async fn current_season(
    State(state): State<AppState>,
) -> Result<Json<CurrentSeasonResponse>, StatusCode> {
    let season = active_season(&state.db).await?.ok_or(StatusCode::NOT_FOUND)?;

    let tiers = sqlx::query_as::<_, TierRow>(
        r#"
        SELECT id, tier, xp_required, free_reward, premium_reward
        FROM bp_tiers
        WHERE season_id = $1
        ORDER BY tier ASC
        "#,
    )
    .bind(season.id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(CurrentSeasonResponse {
        id: season.id,
        name: season.name,
        description: season.description,
        starts_at: season.starts_at,
        ends_at: season.ends_at,
        premium_cost: season.premium_cost,
        premium_currency: season.premium_currency,
        tiers,
    }))
}
