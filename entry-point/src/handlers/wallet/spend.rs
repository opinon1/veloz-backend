//! Placeholder "spend points" endpoint. Lets a client deduct any currency
//! from the authed user's wallet for whatever reason the frontend encodes.
//! Real spend flows (skin purchase, store purchase, BP unlock) use their own
//! endpoints — this is the generic fallback for prototyping.

use axum::{extract::State, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use crate::state::AppState;
use crate::extractors::Claims;
use super::utils::adjust_balance_oneshot;

#[derive(Deserialize)]
pub struct SpendRequest {
    pub currency: String,     // 'high' | 'soft' | 'energy'
    pub amount: i64,          // must be > 0
    pub reason: Option<String>,
    pub reference_id: Option<String>,
}

#[derive(Serialize)]
pub struct SpendResponse {
    pub currency: String,
    pub new_balance: i64,
}

pub async fn spend(
    State(state): State<AppState>,
    Claims(session): Claims,
    Json(payload): Json<SpendRequest>,
) -> Result<Json<SpendResponse>, StatusCode> {
    if payload.amount <= 0 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let reason = payload.reason.as_deref().unwrap_or("spend");
    let new_balance = adjust_balance_oneshot(
        &state.db,
        session.user_id,
        &payload.currency,
        -payload.amount,
        reason,
        payload.reference_id.as_deref(),
    )
    .await?;

    Ok(Json(SpendResponse {
        currency: payload.currency,
        new_balance,
    }))
}
