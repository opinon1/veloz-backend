use axum::{extract::State, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::Claims;

/// PATCH /profile is intentionally narrow: avatar and frame selection live
/// behind dedicated `/avatars/{id}/select` and `/frames/{id}/select`
/// endpoints (which validate ownership). Right now there are no other
/// user-mutable profile fields, so this handler accepts an empty body and
/// echoes the current selections back.
#[derive(Deserialize, Default)]
pub struct UpdateProfileRequest {}

#[derive(Serialize)]
pub struct UpdateProfileResponse {
    pub avatar_url: Option<Uuid>,
    pub frame_url: Option<Uuid>,
}

pub async fn update_profile(
    State(state): State<AppState>,
    Claims(session): Claims,
    Json(_payload): Json<UpdateProfileRequest>,
) -> Result<Json<UpdateProfileResponse>, StatusCode> {
    let row: (Option<Uuid>, Option<Uuid>) = sqlx::query_as(
        "SELECT avatar_url, frame_url FROM profiles WHERE user_id = $1",
    )
    .bind(session.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(UpdateProfileResponse {
        avatar_url: row.0,
        frame_url: row.1,
    }))
}
