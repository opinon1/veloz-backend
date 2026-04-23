use axum::{extract::State, Json, http::StatusCode};
use serde::Serialize;
use uuid::Uuid;
use crate::state::AppState;

#[derive(Serialize, sqlx::FromRow)]
pub struct StoreItem {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub item_type: String,
    pub cost: i64,
    pub currency: String,
    pub iap_product_id: Option<String>,
    pub payload: serde_json::Value,
    pub metadata: serde_json::Value,
}

pub async fn list_items(
    State(state): State<AppState>,
) -> Result<Json<Vec<StoreItem>>, StatusCode> {
    let rows = sqlx::query_as::<_, StoreItem>(
        r#"
        SELECT id, name, description, item_type, cost, currency, iap_product_id, payload, metadata
        FROM store_items
        WHERE is_active = TRUE
        ORDER BY item_type ASC, cost ASC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(rows))
}
