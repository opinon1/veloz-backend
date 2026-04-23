//! Generic store purchase. Charges the authed user for a store_item in whatever
//! currency the item declares (high/soft/energy), applies `profile.price_multiplier`,
//! and fulfills the item server-side based on `item_type` + `payload`.
//!
//! `currency = 'iap'` items are NOT handled here — those go through `/wallet/iap/purchase`
//! for receipt verification.

use axum::{extract::{Path, State}, Json, http::StatusCode};
use serde::Serialize;
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::Claims;
use crate::handlers::wallet::utils::adjust_balance;

#[derive(Serialize)]
pub struct PurchaseItemResponse {
    pub item_id: Uuid,
    pub cost_paid: i64,
    pub currency_paid: String,
    pub new_balance: i64,
    pub payload: serde_json::Value,
}

pub async fn purchase_item(
    State(state): State<AppState>,
    Claims(session): Claims,
    Path(item_id): Path<Uuid>,
) -> Result<Json<PurchaseItemResponse>, StatusCode> {
    let mut tx = state.db.begin().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let item: Option<(i64, String, String, serde_json::Value, bool)> = sqlx::query_as(
        "SELECT cost, currency, item_type, payload, is_active FROM store_items WHERE id = $1",
    )
    .bind(item_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (cost, currency, item_type, payload, is_active) = item.ok_or(StatusCode::NOT_FOUND)?;
    if !is_active {
        return Err(StatusCode::GONE);
    }
    if currency == "iap" {
        return Err(StatusCode::BAD_REQUEST); // route via /wallet/iap/purchase
    }

    // Apply per-user price multiplier.
    let multiplier: (f64,) = sqlx::query_as(
        "SELECT price_multiplier FROM profiles WHERE user_id = $1",
    )
    .bind(session.user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let adjusted_cost = ((cost as f64) * multiplier.0).round() as i64;

    let new_balance = if adjusted_cost > 0 {
        adjust_balance(
            &mut tx,
            session.user_id,
            &currency,
            -adjusted_cost,
            "store_purchase",
            Some(&item_id.to_string()),
        )
        .await?
    } else {
        0
    };

    sqlx::query(
        r#"INSERT INTO store_purchases (user_id, item_id, cost_paid, currency_paid)
           VALUES ($1, $2, $3, $4)"#,
    )
    .bind(session.user_id)
    .bind(item_id)
    .bind(adjusted_cost)
    .bind(&currency)
    .execute(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Fulfillment based on item_type.
    match item_type.as_str() {
        "skin" => {
            if let Some(skin_id_str) = payload.get("skin_id").and_then(|v| v.as_str()) {
                if let Ok(skin_id) = Uuid::parse_str(skin_id_str) {
                    sqlx::query(
                        "INSERT INTO user_skins (user_id, skin_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
                    )
                    .bind(session.user_id)
                    .bind(skin_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                }
            }
        }
        "currency_bundle" => {
            // payload: { "high": 100 } or { "soft": 500, "energy": 10 }
            for cur in ["high", "soft", "energy"] {
                if let Some(amt) = payload.get(cur).and_then(|v| v.as_i64()) {
                    if amt > 0 {
                        adjust_balance(
                            &mut tx,
                            session.user_id,
                            cur,
                            amt,
                            "store_bundle",
                            Some(&item_id.to_string()),
                        )
                        .await?;
                    }
                }
            }
        }
        "energy_refill" => {
            let amt = payload.get("energy").and_then(|v| v.as_i64()).unwrap_or(0);
            if amt > 0 {
                adjust_balance(
                    &mut tx,
                    session.user_id,
                    "energy",
                    amt,
                    "energy_refill",
                    Some(&item_id.to_string()),
                )
                .await?;
            }
        }
        "bp_unlock" | "frame" | "custom" => {
            // Frontend handles via payload/metadata; no server-side fulfillment yet.
        }
        _ => {}
    }

    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(PurchaseItemResponse {
        item_id,
        cost_paid: adjusted_cost,
        currency_paid: currency,
        new_balance,
        payload,
    }))
}
