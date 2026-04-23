use axum::{extract::State, Json, http::StatusCode};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::Claims;

#[derive(Serialize, sqlx::FromRow)]
pub struct OwnedSkinRow {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub outfit_url: String,
    pub acquired_at: DateTime<Utc>,
}

pub async fn owned_skins(
    State(state): State<AppState>,
    Claims(session): Claims,
) -> Result<Json<Vec<OwnedSkinRow>>, StatusCode> {
    let rows = sqlx::query_as::<_, OwnedSkinRow>(
        r#"
        SELECT s.id, s.name, s.description, s.outfit_url, us.acquired_at
        FROM user_skins us
        JOIN skins s ON s.id = us.skin_id
        WHERE us.user_id = $1
        ORDER BY us.acquired_at DESC
        "#,
    )
    .bind(session.user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(rows))
}
