use axum::{extract::State, Json, http::StatusCode};
use serde::Serialize;
use uuid::Uuid;
use crate::state::AppState;

#[derive(Serialize, sqlx::FromRow)]
pub struct SkinRow {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub outfit_url: String,
    pub cost: i64,
    pub currency: String,
    pub is_default: bool,
    pub metadata: serde_json::Value,
}

pub async fn list_skins(
    State(state): State<AppState>,
) -> Result<Json<Vec<SkinRow>>, StatusCode> {
    let rows = sqlx::query_as::<_, SkinRow>(
        r#"
        SELECT id, name, description, outfit_url, cost, currency, is_default, metadata
        FROM skins
        WHERE is_active = TRUE
        ORDER BY cost ASC, name ASC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(rows))
}
