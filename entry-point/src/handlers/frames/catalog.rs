use axum::{extract::State, Json, http::StatusCode};
use serde::Serialize;
use uuid::Uuid;
use crate::state::AppState;

#[derive(Serialize, sqlx::FromRow)]
pub struct FrameRow {
    pub id: Uuid,
    pub name: String,
    pub price: i64,
    pub currency: String,
}

pub async fn catalog_frames(
    State(state): State<AppState>,
) -> Result<Json<Vec<FrameRow>>, StatusCode> {
    let rows = sqlx::query_as::<_, FrameRow>(
        r#"
        SELECT id, name, price, currency
        FROM frames
        WHERE is_active = TRUE
        ORDER BY price ASC, name ASC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(rows))
}
