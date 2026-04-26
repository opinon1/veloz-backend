use axum::{extract::State, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
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
    // If the client sends a UUID-shaped avatar_url, require the user to own
    // the referenced skin. Without this check a user could bypass the
    // ownership gate on /skins/{id}/equip by PATCHing the avatar directly.
    // Non-UUID strings are still allowed (legacy non-skin avatar URLs).
    if let Some(ref url) = payload.avatar_url {
        if !url.is_empty() {
            if let Ok(skin_id) = Uuid::parse_str(url) {
                let owned: Option<(Uuid,)> = sqlx::query_as(
                    "SELECT user_id FROM user_skins WHERE user_id = $1 AND skin_id = $2",
                )
                .bind(session.user_id)
                .bind(skin_id)
                .fetch_optional(&state.db)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                if owned.is_none() {
                    return Err(StatusCode::FORBIDDEN);
                }
            }
        }
    }

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
