use axum::{extract::State, Json, http::StatusCode};
use serde::Serialize;
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::Claims;

#[derive(Serialize)]
pub struct OwnedFrameRow {
    pub id: Uuid,
    pub is_selected: bool,
}

pub async fn owned_frames(
    State(state): State<AppState>,
    Claims(session): Claims,
) -> Result<Json<Vec<OwnedFrameRow>>, StatusCode> {
    let selected: Option<(Option<Uuid>,)> = sqlx::query_as(
        "SELECT frame_url FROM profiles WHERE user_id = $1",
    )
    .bind(session.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let selected_id = selected.and_then(|(s,)| s);

    let ids: Vec<(Uuid,)> = sqlx::query_as(
        r#"
        SELECT f.id
        FROM user_frames uf
        JOIN frames f ON f.id = uf.frame_id
        WHERE uf.user_id = $1 AND f.is_active = TRUE
        ORDER BY uf.acquired_at DESC
        "#,
    )
    .bind(session.user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rows = ids
        .into_iter()
        .map(|(id,)| OwnedFrameRow {
            id,
            is_selected: selected_id == Some(id),
        })
        .collect();

    Ok(Json(rows))
}
