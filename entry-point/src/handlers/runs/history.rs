use axum::{extract::{Query, State}, Json, http::StatusCode};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::Claims;

#[derive(Deserialize)]
pub struct HistoryQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
}
fn default_limit() -> i64 { 25 }

#[derive(Serialize, sqlx::FromRow)]
pub struct RunRow {
    pub id: Uuid,
    pub score: i64,
    pub distance: i64,
    pub coins_collected: i64,
    pub duration_ms: i64,
    pub xp_awarded: i64,
    pub bp_xp_awarded: i64,
    pub created_at: DateTime<Utc>,
}

pub async fn my_history(
    State(state): State<AppState>,
    Claims(session): Claims,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<Vec<RunRow>>, StatusCode> {
    let limit = q.limit.clamp(1, 100);

    let rows = sqlx::query_as::<_, RunRow>(
        r#"
        SELECT id, score, distance, coins_collected, duration_ms, xp_awarded, bp_xp_awarded, created_at
        FROM runs
        WHERE user_id = $1
        ORDER BY created_at DESC
        LIMIT $2
        "#,
    )
    .bind(session.user_id)
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(rows))
}
