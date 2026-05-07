use axum::{extract::{Path, State}, Json, http::StatusCode};
use serde::Serialize;
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::Claims;

#[derive(Serialize)]
pub struct SelectFrameResponse {
    pub frame_id: Option<Uuid>,
}

pub async fn select_frame(
    State(state): State<AppState>,
    Claims(session): Claims,
    Path(frame_id): Path<Uuid>,
) -> Result<Json<SelectFrameResponse>, StatusCode> {
    let owned: Option<(Uuid,)> = sqlx::query_as(
        "SELECT user_id FROM user_frames WHERE user_id = $1 AND frame_id = $2",
    )
    .bind(session.user_id)
    .bind(frame_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if owned.is_none() {
        return Err(StatusCode::FORBIDDEN);
    }

    sqlx::query(
        "UPDATE profiles SET frame_url = $2, updated_at = CURRENT_TIMESTAMP WHERE user_id = $1",
    )
    .bind(session.user_id)
    .bind(frame_id)
    .execute(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(SelectFrameResponse {
        frame_id: Some(frame_id),
    }))
}

pub async fn deselect_frame(
    State(state): State<AppState>,
    Claims(session): Claims,
) -> Result<Json<SelectFrameResponse>, StatusCode> {
    sqlx::query(
        "UPDATE profiles SET frame_url = NULL, updated_at = CURRENT_TIMESTAMP WHERE user_id = $1",
    )
    .bind(session.user_id)
    .execute(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(SelectFrameResponse { frame_id: None }))
}
