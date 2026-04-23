use axum::{extract::{Query, State}, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct LeaderboardQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
}
fn default_limit() -> i64 { 50 }

#[derive(Serialize, sqlx::FromRow)]
pub struct LeaderboardRow {
    pub user_id: Uuid,
    pub username: String,
    pub main_highscore: i64,
    pub account_level: i32,
    pub avatar_url: Option<String>,
    pub frame_url: Option<String>,
}

pub async fn leaderboard(
    State(state): State<AppState>,
    Query(q): Query<LeaderboardQuery>,
) -> Result<Json<Vec<LeaderboardRow>>, StatusCode> {
    let limit = q.limit.clamp(1, 200);

    let rows = sqlx::query_as::<_, LeaderboardRow>(
        r#"
        SELECT u.id AS user_id, u.username, p.main_highscore, p.account_level, p.avatar_url, p.frame_url
        FROM profiles p
        JOIN users u ON u.id = p.user_id
        WHERE u.is_active = TRUE AND p.main_highscore > 0
        ORDER BY p.main_highscore DESC, u.username ASC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(rows))
}
