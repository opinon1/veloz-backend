use axum::{extract::State, Json, http::StatusCode};
use serde::Serialize;
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::Claims;

#[derive(Serialize, sqlx::FromRow)]
pub struct ProfileResponse {
    pub user_id: Uuid,
    pub username: String,
    pub email: String,
    pub account_level: i32,
    pub total_xp: i64,
    pub price_multiplier: f64,
    pub main_highscore: i64,
    pub avatar_url: Option<String>,
    pub frame_url: Option<String>,
}

pub async fn get_profile(
    State(state): State<AppState>,
    Claims(session): Claims,
) -> Result<Json<ProfileResponse>, StatusCode> {
    let row = sqlx::query_as::<_, ProfileResponse>(
        r#"
        SELECT
            u.id AS user_id,
            u.username,
            u.email,
            p.account_level,
            p.total_xp,
            p.price_multiplier,
            p.main_highscore,
            p.avatar_url,
            p.frame_url
        FROM users u
        JOIN profiles p ON p.user_id = u.id
        WHERE u.id = $1
        "#,
    )
    .bind(session.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(row))
}
