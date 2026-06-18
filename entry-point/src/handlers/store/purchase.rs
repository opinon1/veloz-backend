//! Generic store purchase. Charges the authed user `cost * price_multiplier`
//! in the item's wallet currency, then iterates the item's payload (an array
//! of `Grant`s) and applies each one in the same transaction.
//!
//! `currency = "iap"` items don't go through this endpoint — they require
//! receipt validation and are fulfilled by `/wallet/iap/purchase`.

use crate::extractors::Claims;
use crate::handlers::grants_util::apply_grant;
use crate::handlers::missions::service::{MissionEvent, record_event};
use crate::handlers::wallet::utils::adjust_balance;
use crate::models::store_types::{Grant, StoreCurrency, validate_grants};
use crate::pricing::apply_dynamic_price;
use crate::state::AppState;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Serialize;
use std::str::FromStr;
use uuid::Uuid;

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
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let item: Option<(i64, String, serde_json::Value, bool, String, String)> = sqlx::query_as(
        "SELECT cost, currency, payload, is_active, item_type, name FROM store_items WHERE id = $1",
    )
    .bind(item_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (cost, currency_str, payload, is_active, item_type, item_name) =
        item.ok_or(StatusCode::NOT_FOUND)?;
    if !is_active {
        return Err(StatusCode::GONE);
    }

    let currency =
        StoreCurrency::from_str(&currency_str).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    // IAP-priced items must go through /wallet/iap/purchase, not this endpoint.
    let wallet_currency = currency
        .as_wallet_currency()
        .ok_or(StatusCode::BAD_REQUEST)?;

    // Parse the persisted payload into a Vec<Grant>. Admin-time validation
    // already guaranteed this — fall through to 500 only if the row was
    // tampered with after creation.
    let grants = validate_grants(&payload).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Dynamic per-(user, item) price stacked on top of the admin's
    // flat `profiles.price_multiplier` discount. See `crate::pricing`
    // for the linear-plus-sine formula. At total_xp = 0 the dynamic
    // factor collapses to 1.0, so a fresh user still pays exact base.
    let (total_xp, account_multiplier): (i64, f64) = sqlx::query_as(
        "SELECT total_xp, price_multiplier FROM profiles WHERE user_id = $1",
    )
    .bind(session.user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let adjusted_cost = apply_dynamic_price(
        cost,
        session.user_id,
        item_id,
        total_xp,
        account_multiplier,
    );

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
    for g in &grants {
        apply_grant(&mut tx, session.user_id, g, "store_grant", item_id).await?;
    }

    // Mission hooks: one StorePurchase event, plus a CurrencyCollected
    // event for each currency grant in the payload (matches how runs
    // credit the wallet directly).
    record_event(
        &mut tx,
        session.user_id,
        MissionEvent::StorePurchase {
            item_type: item_type.clone(),
        },
    )
    .await?;
    for g in &grants {
        if let Grant::Currency { currency, amount } = g {
            record_event(
                &mut tx,
                session.user_id,
                MissionEvent::CurrencyCollected {
                    currency: currency.as_str().to_string(),
                    amount: *amount,
                },
            )
            .await?;
        }
    }

    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Fire-and-forget purchase receipt. Enqueued after commit so the email is
    // never sent for a rolled-back purchase; failure here can't affect the 200.
    crate::services::mailer::dispatch_to_user(
        &state,
        session.user_id,
        crate::services::mailer::EmailKind::PurchaseReceipt {
            item_name,
            cost: adjusted_cost,
            currency: wallet_currency.as_str().to_string(),
            new_balance,
        },
    );

    Ok(Json(PurchaseItemResponse {
        item_id,
        cost_paid: adjusted_cost,
        currency_paid: wallet_currency.as_str().to_string(),
        new_balance,
        payload,
    }))
}
