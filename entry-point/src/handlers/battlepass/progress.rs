use axum::{extract::State, Json, http::StatusCode};
use serde::Serialize;
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::Claims;
use super::utils::active_season;

#[derive(Serialize)]
pub struct ProgressResponse {
    pub season_id: Uuid,
    pub bp_xp: i64,
    pub premium_unlocked: bool,
    pub claimed_free: Vec<i32>,
    pub claimed_premium: Vec<i32>,
}

pub async fn my_progress(
    State(state): State<AppState>,
    Claims(session): Claims,
) -> Result<Json<ProgressResponse>, StatusCode> {
    let season = active_season(&state.db).await?.ok_or(StatusCode::NOT_FOUND)?;

    let row: Option<(i64, bool)> = sqlx::query_as(
        "SELECT bp_xp, premium_unlocked FROM bp_progress WHERE user_id = $1 AND season_id = $2",
    )
    .bind(session.user_id)
    .bind(season.id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (bp_xp, premium_unlocked) = row.unwrap_or((0, false));

    let claims: Vec<(i32, String)> = sqlx::query_as(
        "SELECT tier, track FROM bp_claims WHERE user_id = $1 AND season_id = $2",
    )
    .bind(session.user_id)
    .bind(season.id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let claimed_free = claims.iter().filter(|(_, t)| t == "free").map(|(t, _)| *t).collect();
    let claimed_premium = claims.iter().filter(|(_, t)| t == "premium").map(|(t, _)| *t).collect();

    Ok(Json(ProgressResponse {
        season_id: season.id,
        bp_xp,
        premium_unlocked,
        claimed_free,
        claimed_premium,
    }))
}
