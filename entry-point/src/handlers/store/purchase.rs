//! Generic store purchase. Charges the authed user `cost * price_multiplier`
//! in the item's wallet currency, then iterates the item's payload (an array
//! of `Grant`s) and applies each one in the same transaction.
//!
//! `currency = "iap"` items don't go through this endpoint — they require
//! receipt validation and are fulfilled by `/wallet/iap/purchase`.

use axum::{extract::{Path, State}, Json, http::StatusCode};
use serde::Serialize;
use std::str::FromStr;
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::Claims;
use crate::handlers::wallet::utils::adjust_balance;
use crate::models::store_types::{Grant, StoreCurrency, validate_grants};

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

    let item: Option<(i64, String, serde_json::Value, bool)> = sqlx::query_as(
        "SELECT cost, currency, payload, is_active FROM store_items WHERE id = $1",
    )
    .bind(item_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (cost, currency_str, payload, is_active) = item.ok_or(StatusCode::NOT_FOUND)?;
    if !is_active {
        return Err(StatusCode::GONE);
    }

    let currency = StoreCurrency::from_str(&currency_str)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    // IAP-priced items must go through /wallet/iap/purchase, not this endpoint.
    let wallet_currency = currency.as_wallet_currency().ok_or(StatusCode::BAD_REQUEST)?;

    // Parse the persisted payload into a Vec<Grant>. Admin-time validation
    // already guaranteed this — fall through to 500 only if the row was
    // tampered with after creation.
    let grants = validate_grants(&payload).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Apply per-user price multiplier. Defense-in-depth clamp at >= 0 in case
    // a negative multiplier slipped past the admin-side guard.
    let multiplier: (f64,) = sqlx::query_as(
        "SELECT price_multiplier FROM profiles WHERE user_id = $1",
    )
    .bind(session.user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

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
        // Free / multiplier-zeroed purchase: return actual current balance
        // for the spend currency so clients render accurate UI.
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

    // Fulfillment: iterate the grants array and apply each in the same tx.
    // Adding a new Grant variant produces a compile error in `apply_grant`,
    // forcing the developer to choose how it's fulfilled.
    for g in &grants {
        apply_grant(&mut tx, session.user_id, g, item_id).await?;
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

/// Apply one Grant atomically inside the caller's transaction. Idempotent
/// where the underlying schema permits (e.g. user_skins ON CONFLICT DO NOTHING).
async fn apply_grant(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: Uuid,
    grant: &Grant,
    reference_id: Uuid,
) -> Result<(), StatusCode> {
    match grant {
        Grant::Currency { currency, amount } => {
            adjust_balance(
                tx,
                user_id,
                currency.as_str(),
                *amount,
                "store_grant",
                Some(&reference_id.to_string()),
            )
            .await?;
        }
        Grant::Skin { skin_id } => {
            sqlx::query(
                "INSERT INTO user_skins (user_id, skin_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(user_id)
            .bind(skin_id)
            .execute(&mut **tx)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }
    Ok(())
}
