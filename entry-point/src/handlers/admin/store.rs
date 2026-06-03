use axum::{extract::{Path, State}, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::AdminClaims;
use crate::models::store_types::{ItemType, StoreCurrency, validate_grants};

/// Validates a store item's payload structure. The payload is now an array of
/// `Grant`s (`{"type": "currency", ...} | {"type": "skin", ...}`), so a
/// single store item can ship multiple things at once (e.g. skin + 100 soft).
///
/// `item_type` is now informational (admin-defined display category) and no
/// longer drives fulfillment — the payload array does. We still validate that
/// the *kind* declared via `item_type` and `currency` is internally consistent
/// (IAP needs a product id; non-IAP needs a wallet currency).
fn validate_store_payload(
    _item_type: ItemType,
    currency: StoreCurrency,
    iap_product_id: Option<&str>,
    payload: &serde_json::Value,
) -> Result<(), StatusCode> {
    if matches!(currency, StoreCurrency::Iap)
        && iap_product_id.map(str::is_empty).unwrap_or(true)
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    validate_grants(payload).map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(())
}

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
    /// If true, every newly-signed-up user has this item's payload
    /// Grants applied automatically (currency credits, skin unlocks,
    /// …). IAP-priced items can also be marked default — the payload
    /// is fulfilled the same way without going through Etomin.
    #[serde(default)]
    pub is_default: bool,
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
    pub is_default: bool,
}

pub async fn create_item(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Json(payload): Json<CreateItemRequest>,
) -> Result<(StatusCode, Json<StoreItemRow>), StatusCode> {
    if payload.cost < 0 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let item_type = ItemType::from_str(&payload.item_type)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let currency = StoreCurrency::from_str(&payload.currency)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    validate_store_payload(
        item_type,
        currency,
        payload.iap_product_id.as_deref(),
        &payload.payload,
    )?;

    let row = sqlx::query_as::<_, StoreItemRow>(
        r#"
        INSERT INTO store_items (name, description, item_type, cost, currency, iap_product_id, payload, metadata, is_default)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING id, name, description, item_type, cost, currency, iap_product_id, payload, metadata, is_active, is_default
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
    .bind(payload.is_default)
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
        "SELECT id, name, description, item_type, cost, currency, iap_product_id, payload, metadata, is_active, is_default FROM store_items ORDER BY created_at DESC",
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
    pub is_default: Option<bool>,
}

pub async fn update_item(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateItemRequest>,
) -> Result<Json<StoreItemRow>, StatusCode> {
    if let Some(c) = payload.cost {
        if c < 0 {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    // If the update touches item_type/currency/payload/iap_product_id, we need
    // the full effective tuple to validate. Merge the provided fields with the
    // current row so partial updates don't bypass validation.
    if payload.item_type.is_some()
        || payload.currency.is_some()
        || payload.payload.is_some()
        || payload.iap_product_id.is_some()
    {
        let current: Option<(String, String, Option<String>, serde_json::Value)> = sqlx::query_as(
            "SELECT item_type, currency, iap_product_id, payload FROM store_items WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let (cur_type, cur_currency, cur_iap, cur_payload) =
            current.ok_or(StatusCode::NOT_FOUND)?;
        let effective_type_str = payload.item_type.as_deref().unwrap_or(&cur_type);
        let effective_currency_str = payload.currency.as_deref().unwrap_or(&cur_currency);
        let effective_iap = payload
            .iap_product_id
            .as_deref()
            .or(cur_iap.as_deref());
        let effective_payload = payload.payload.as_ref().unwrap_or(&cur_payload);
        let effective_type = ItemType::from_str(effective_type_str)
            .map_err(|_| StatusCode::BAD_REQUEST)?;
        let effective_currency = StoreCurrency::from_str(effective_currency_str)
            .map_err(|_| StatusCode::BAD_REQUEST)?;
        validate_store_payload(effective_type, effective_currency, effective_iap, effective_payload)?;
    }

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
            is_default      = COALESCE($11, is_default),
            updated_at      = CURRENT_TIMESTAMP
        WHERE id = $1
        RETURNING id, name, description, item_type, cost, currency, iap_product_id, payload, metadata, is_active, is_default
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
    .bind(payload.is_default)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(row))
}

/// Hard DELETE only works when the item has never been bought or paid for.
/// `store_purchases.item_id` and `payments.item_id` both FK here; the
/// payments one is explicitly ON DELETE RESTRICT, the purchases one defaults
/// to NO ACTION. Either way Postgres blocks the delete with 23503 once any
/// referencing row exists. We map that to 409 + a hint to use
/// `PATCH /admin/store/{id}` with `is_active=false` instead.
pub async fn delete_item(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let result = sqlx::query("DELETE FROM store_items WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(db) if db.code().as_deref() == Some("23503") => {
                StatusCode::CONFLICT
            }
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        })?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}
