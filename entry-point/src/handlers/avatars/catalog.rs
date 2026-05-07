use axum::{extract::State, Json, http::StatusCode};
use serde::Serialize;
use uuid::Uuid;
use crate::state::AppState;

#[derive(Serialize, sqlx::FromRow)]
pub struct AvatarRow {
    pub id: Uuid,
    pub name: String,
    pub price: i64,
    pub currency: String,
}

/// Public listing of every active avatar — backs the purchase UI.
pub async fn catalog_avatars(
    State(state): State<AppState>,
) -> Result<Json<Vec<AvatarRow>>, StatusCode> {
    let rows = sqlx::query_as::<_, AvatarRow>(
        r#"
        SELECT id, name, price, currency
        FROM avatars
        WHERE is_active = TRUE
        ORDER BY price ASC, name ASC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(rows))
}
