//! Generic store purchase. Charges the authed user for a store_item in whatever
//! currency the item declares (high/soft/energy), applies `profile.price_multiplier`,
//! and fulfills the item server-side based on `item_type` + `payload`.
//!
//! `currency = 'iap'` items are NOT handled here — those go through `/wallet/iap/purchase`
//! for receipt verification.

use axum::{extract::{Path, State}, Json, http::StatusCode};
use serde::Serialize;
use std::str::FromStr;
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::Claims;
use crate::handlers::wallet::utils::adjust_balance;
use crate::models::store_types::{Currency, ItemType, StoreCurrency};

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

    let (cost, currency_str, item_type_str, payload, is_active) =
        item.ok_or(StatusCode::NOT_FOUND)?;
    if !is_active {
        return Err(StatusCode::GONE);
    }

    // Parse the persisted text into typed enums. validate_store_payload at
    // admin-time guarantees these always parse, but treat unexpected DB rows
    // as 500 rather than panicking.
    let currency = StoreCurrency::from_str(&currency_str)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let item_type = ItemType::from_str(&item_type_str)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // IAP-priced items must go through /wallet/iap/purchase, not this endpoint.
    let wallet_currency = currency.as_wallet_currency().ok_or(StatusCode::BAD_REQUEST)?;

    // Apply per-user price multiplier.
    let multiplier: (f64,) = sqlx::query_as(
        "SELECT price_multiplier FROM profiles WHERE user_id = $1",
    )
    .bind(session.user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Defense in depth against an admin saving a negative price_multiplier.
    // Admin update validates this server-side, but clamp here as well so a
    // negative multiplier can never *grant* the user money during purchase.
    let raw_cost = (cost as f64) * multiplier.0;
    let adjusted_cost = if raw_cost.is_finite() {
        raw_cost.round().max(0.0) as i64
    } else {
        0
    };

    let new_balance = if adjusted_cost > 0 {
        adjust_balance(
            &mut tx,
            session.user_id,
            wallet_currency.as_str(),
            -adjusted_cost,
            "store_purchase",
            Some(&item_id.to_string()),
        )
        .await?
    } else {
        // Free / multiplier-zeroed purchase. Return actual current balance
        // for the spend currency so clients render correct UI.
        let q = format!(
            "SELECT {col} FROM wallets WHERE user_id = $1",
            col = wallet_currency.as_str()
        );
        let (bal,): (i64,) = sqlx::query_as(&q)
            .bind(session.user_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        bal
    };

    sqlx::query(
        r#"INSERT INTO store_purchases (user_id, item_id, cost_paid, currency_paid)
           VALUES ($1, $2, $3, $4)"#,
    )
    .bind(session.user_id)
    .bind(item_id)
    .bind(adjusted_cost)
    .bind(wallet_currency.as_str())
    .execute(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Fulfillment dispatched on the typed item_type. Adding a new ItemType
    // variant produces a non-exhaustive-match compile error here, forcing
    // the developer to make a deliberate choice (real fulfillment vs. opaque).
    match item_type {
        ItemType::Skin => {
            // Validation guarantees skin_id is present and parses; INTERNAL on
            // anything else (would mean the row was tampered with post-create).
            let skin_id_str = payload
                .get("skin_id")
                .and_then(|v| v.as_str())
                .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
            let skin_id = Uuid::parse_str(skin_id_str)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            sqlx::query(
                "INSERT INTO user_skins (user_id, skin_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(session.user_id)
            .bind(skin_id)
            .execute(&mut *tx)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        ItemType::CurrencyBundle => {
            // payload: { "high": 100 } or { "soft": 500, "energy": 10 }
            for cur in [Currency::High, Currency::Soft, Currency::Energy] {
                if let Some(amt) = payload.get(cur.as_str()).and_then(|v| v.as_i64()) {
                    if amt > 0 {
                        adjust_balance(
                            &mut tx,
                            session.user_id,
                            cur.as_str(),
                            amt,
                            "store_bundle",
                            Some(&item_id.to_string()),
                        )
                        .await?;
                    }
                }
            }
        }
        ItemType::EnergyRefill => {
            let amt = payload.get("energy").and_then(|v| v.as_i64()).unwrap_or(0);
            if amt > 0 {
                adjust_balance(
                    &mut tx,
                    session.user_id,
                    Currency::Energy.as_str(),
                    amt,
                    "energy_refill",
                    Some(&item_id.to_string()),
                )
                .await?;
            }
        }
        ItemType::Frame | ItemType::BpUnlock | ItemType::Custom => {
            // Frontend handles via payload/metadata; no server-side fulfillment.
        }
    }

    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(PurchaseItemResponse {
        item_id,
        cost_paid: adjusted_cost,
        currency_paid: wallet_currency.as_str().to_string(),
        new_balance,
        payload,
    }))
}
