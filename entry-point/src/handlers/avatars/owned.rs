use axum::{extract::State, Json, http::StatusCode};
use serde::Serialize;
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::Claims;

#[derive(Serialize)]
pub struct OwnedAvatarRow {
    pub id: Uuid,
    pub is_selected: bool,
}

/// Spec: GET /avatars returns the user's *unlocked* avatars only, each with
/// `is_selected = true` iff it matches `profiles.avatar_url`.
pub async fn owned_avatars(
    State(state): State<AppState>,
    Claims(session): Claims,
) -> Result<Json<Vec<OwnedAvatarRow>>, StatusCode> {
    let selected: Option<(Option<Uuid>,)> = sqlx::query_as(
        "SELECT avatar_url FROM profiles WHERE user_id = $1",
    )
    .bind(session.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let selected_id = selected.and_then(|(s,)| s);

    let ids: Vec<(Uuid,)> = sqlx::query_as(
        r#"
        SELECT a.id
        FROM user_avatars ua
        JOIN avatars a ON a.id = ua.avatar_id
        WHERE ua.user_id = $1 AND a.is_active = TRUE
        ORDER BY ua.acquired_at DESC
        "#,
    )
    .bind(session.user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rows = ids
        .into_iter()
        .map(|(id,)| OwnedAvatarRow {
            id,
            is_selected: selected_id == Some(id),
        })
        .collect();

    Ok(Json(rows))
}
