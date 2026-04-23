use axum::{extract::State, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use crate::state::AppState;
use crate::extractors::Claims;

#[derive(Deserialize)]
pub struct UpdateProfileRequest {
    /// Typically a skin_id (UUID string) but stored opaquely as text for flexibility.
    pub avatar_url: Option<String>,
    pub frame_url: Option<String>,
}

#[derive(Serialize)]
pub struct UpdateProfileResponse {
    pub avatar_url: Option<String>,
    pub frame_url: Option<String>,
}

pub async fn update_profile(
    State(state): State<AppState>,
    Claims(session): Claims,
    Json(payload): Json<UpdateProfileRequest>,
) -> Result<Json<UpdateProfileResponse>, StatusCode> {
    let row: (Option<String>, Option<String>) = sqlx::query_as(
        r#"
        UPDATE profiles
        SET
            avatar_url = COALESCE($2, avatar_url),
            frame_url  = COALESCE($3, frame_url),
            updated_at = CURRENT_TIMESTAMP
        WHERE user_id = $1
        RETURNING avatar_url, frame_url
        "#,
    )
    .bind(session.user_id)
    .bind(&payload.avatar_url)
    .bind(&payload.frame_url)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(UpdateProfileResponse {
        avatar_url: row.0,
        frame_url: row.1,
    }))
}
