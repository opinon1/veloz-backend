//! Placeholder IAP endpoints. Replace the TODO blocks with real Apple/Google
//! receipt verification + fulfillment when ready. Current behavior: accepts any
//! receipt string, logs the intent, returns the claimed grant without crediting.

use axum::{extract::State, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use crate::state::AppState;
use crate::extractors::Claims;

#[derive(Deserialize)]
pub struct PurchaseRequest {
    pub product_id: String,
    pub platform: String,       // 'ios' | 'android' | 'stripe' | ...
    pub receipt: String,        // raw receipt / purchase token
}

#[derive(Serialize)]
pub struct PurchaseResponse {
    pub status: String,
    pub product_id: String,
    /// Echo of what the client thinks was granted — REAL implementation must
    /// look this up server-side from the verified receipt.
    pub pending_grant: serde_json::Value,
}

pub async fn purchase(
    State(_state): State<AppState>,
    Claims(session): Claims,
    Json(payload): Json<PurchaseRequest>,
) -> Result<Json<PurchaseResponse>, StatusCode> {
    tracing::info!(
        user_id = %session.user_id,
        product_id = %payload.product_id,
        platform = %payload.platform,
        "IAP purchase received (placeholder — no fulfillment)"
    );

    // TODO: verify receipt with Apple/Google, look up product -> grant mapping,
    //       adjust wallet inside a transaction, record in wallet_ledger with reason='iap'.

    Ok(Json(PurchaseResponse {
        status: "pending_verification".into(),
        product_id: payload.product_id,
        pending_grant: serde_json::json!({}),
    }))
}

#[derive(Deserialize)]
pub struct ValidateRequest {
    pub product_id: String,
    pub platform: String,
    pub receipt: String,
}

#[derive(Serialize)]
pub struct ValidateResponse {
    pub valid: bool,
    pub product_id: String,
}

pub async fn validate(
    State(_state): State<AppState>,
    Claims(session): Claims,
    Json(payload): Json<ValidateRequest>,
) -> Result<Json<ValidateResponse>, StatusCode> {
    tracing::info!(
        user_id = %session.user_id,
        product_id = %payload.product_id,
        "IAP validate received (placeholder — always returns valid=true)"
    );

    // TODO: real receipt validation against platform stores.

    Ok(Json(ValidateResponse {
        valid: true,
        product_id: payload.product_id,
    }))
}
