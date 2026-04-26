use axum::{extract::{Path, State}, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::AdminClaims;
use crate::models::store_types::{Currency, ItemType, StoreCurrency};

/// Validates a store item's currency + item_type combo and confirms the
/// payload shape matches the declared type. Called on create and update so
/// purchase-time fulfillment can assume the payload is well-formed.
///
/// Fails fast at admin-time (400) instead of silently skipping fulfillment
/// at purchase-time (which would charge the user but grant nothing).
///
/// Inputs are typed enums — adding a new `ItemType` variant produces a
/// compile error here until it's handled.
fn validate_store_payload(
    item_type: ItemType,
    currency: StoreCurrency,
    iap_product_id: Option<&str>,
    payload: &serde_json::Value,
) -> Result<(), StatusCode> {
    if matches!(currency, StoreCurrency::Iap)
        && iap_product_id.map(str::is_empty).unwrap_or(true)
    {
        return Err(StatusCode::BAD_REQUEST);
    }

    match item_type {
        ItemType::Skin => {
            let sid = payload
                .get("skin_id")
                .and_then(|v| v.as_str())
                .ok_or(StatusCode::BAD_REQUEST)?;
            Uuid::parse_str(sid).map_err(|_| StatusCode::BAD_REQUEST)?;
        }
        ItemType::CurrencyBundle => {
            let obj = payload.as_object().ok_or(StatusCode::BAD_REQUEST)?;
            let mut granted_any = false;
            for (k, v) in obj {
                // Each key must parse as a wallet Currency, and the value
                // must be a positive integer.
                Currency::from_str(k).map_err(|_| StatusCode::BAD_REQUEST)?;
                let amt = v.as_i64().ok_or(StatusCode::BAD_REQUEST)?;
                if amt <= 0 {
                    return Err(StatusCode::BAD_REQUEST);
                }
                granted_any = true;
            }
            if !granted_any {
                return Err(StatusCode::BAD_REQUEST);
            }
        }
        ItemType::EnergyRefill => {
            let amt = payload
                .get("energy")
                .and_then(|v| v.as_i64())
                .ok_or(StatusCode::BAD_REQUEST)?;
            if amt <= 0 {
                return Err(StatusCode::BAD_REQUEST);
            }
        }
        ItemType::Frame | ItemType::BpUnlock | ItemType::Custom => {
            // No server-side fulfillment; payload is opaque for the frontend.
        }
    }
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
