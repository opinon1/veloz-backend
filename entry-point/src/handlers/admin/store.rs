use axum::{extract::{Path, State}, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::AdminClaims;

#[derive(Deserialize)]
pub struct CreateItemRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub item_type: String,
    pub cost: i64,
    pub currency: String,
    #[serde(default)]
    pub iap_product_id: Option<String>,
    #[serde(default)]
    pub payload: serde_json::Value,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct StoreItemRow {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub item_type: String,
    pub cost: i64,
    pub currency: String,
    pub iap_product_id: Option<String>,
    pub payload: serde_json::Value,
    pub metadata: serde_json::Value,
    pub is_active: bool,
}

pub async fn create_item(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Json(payload): Json<CreateItemRequest>,
) -> Result<(StatusCode, Json<StoreItemRow>), StatusCode> {
    let row = sqlx::query_as::<_, StoreItemRow>(
        r#"
        INSERT INTO store_items (name, description, item_type, cost, currency, iap_product_id, payload, metadata)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id, name, description, item_type, cost, currency, iap_product_id, payload, metadata, is_active
        "#,
    )
    .bind(&payload.name)
    .bind(&payload.description)
    .bind(&payload.item_type)
    .bind(payload.cost)
    .bind(&payload.currency)
    .bind(&payload.iap_product_id)
    .bind(&payload.payload)
    .bind(&payload.metadata)
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((StatusCode::CREATED, Json(row)))
}

pub async fn list_all_items(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
) -> Result<Json<Vec<StoreItemRow>>, StatusCode> {
    let rows = sqlx::query_as::<_, StoreItemRow>(
        "SELECT id, name, description, item_type, cost, currency, iap_product_id, payload, metadata, is_active FROM store_items ORDER BY created_at DESC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct UpdateItemRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub item_type: Option<String>,
    pub cost: Option<i64>,
    pub currency: Option<String>,
    pub iap_product_id: Option<String>,
    pub payload: Option<serde_json::Value>,
    pub metadata: Option<serde_json::Value>,
    pub is_active: Option<bool>,
}

pub async fn update_item(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateItemRequest>,
) -> Result<Json<StoreItemRow>, StatusCode> {
    let row = sqlx::query_as::<_, StoreItemRow>(
        r#"
        UPDATE store_items SET
            name            = COALESCE($2, name),
            description     = COALESCE($3, description),
            item_type       = COALESCE($4, item_type),
            cost            = COALESCE($5, cost),
            currency        = COALESCE($6, currency),
            iap_product_id  = COALESCE($7, iap_product_id),
            payload         = COALESCE($8, payload),
            metadata        = COALESCE($9, metadata),
            is_active       = COALESCE($10, is_active),
            updated_at      = CURRENT_TIMESTAMP
        WHERE id = $1
        RETURNING id, name, description, item_type, cost, currency, iap_product_id, payload, metadata, is_active
        "#,
    )
    .bind(id)
    .bind(&payload.name)
    .bind(&payload.description)
    .bind(&payload.item_type)
    .bind(payload.cost)
    .bind(&payload.currency)
    .bind(&payload.iap_product_id)
    .bind(&payload.payload)
    .bind(&payload.metadata)
    .bind(payload.is_active)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(row))
}

pub async fn delete_item(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let result = sqlx::query("DELETE FROM store_items WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}
