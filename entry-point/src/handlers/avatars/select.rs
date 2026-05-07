use axum::{extract::{Path, State}, Json, http::StatusCode};
use serde::Serialize;
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::Claims;

#[derive(Serialize)]
pub struct SelectAvatarResponse {
    pub avatar_id: Option<Uuid>,
}

/// POST /avatars/{id}/select — sets `profiles.avatar_url = id`.
/// Requires the user owns the avatar (FORBIDDEN otherwise).
pub async fn select_avatar(
    State(state): State<AppState>,
    Claims(session): Claims,
    Path(avatar_id): Path<Uuid>,
) -> Result<Json<SelectAvatarResponse>, StatusCode> {
    let owned: Option<(Uuid,)> = sqlx::query_as(
        "SELECT user_id FROM user_avatars WHERE user_id = $1 AND avatar_id = $2",
    )
    .bind(session.user_id)
    .bind(avatar_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if owned.is_none() {
        return Err(StatusCode::FORBIDDEN);
    }

    sqlx::query(
        "UPDATE profiles SET avatar_url = $2, updated_at = CURRENT_TIMESTAMP WHERE user_id = $1",
    )
    .bind(session.user_id)
    .bind(avatar_id)
    .execute(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(SelectAvatarResponse {
        avatar_id: Some(avatar_id),
    }))
}

/// POST /avatars/deselect — clears the current selection.
pub async fn deselect_avatar(
    State(state): State<AppState>,
    Claims(session): Claims,
) -> Result<Json<SelectAvatarResponse>, StatusCode> {
    sqlx::query(
        "UPDATE profiles SET avatar_url = NULL, updated_at = CURRENT_TIMESTAMP WHERE user_id = $1",
    )
    .bind(session.user_id)
    .execute(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(SelectAvatarResponse { avatar_id: None }))
}
