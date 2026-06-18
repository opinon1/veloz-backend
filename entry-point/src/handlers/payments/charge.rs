//! POST /payments/charge — kicks off an Etomin /sale call.
//!
//! Flow:
//!   1. Look up the store item; must be currency='iap' and is_active.
//!   2. Insert a payments row in PENDING with our reference id.
//!   3. Call Etomin /sale, passing payment.id as `reference` for idempotency.
//!   4. Inspect Etomin's `status`:
//!        APPROVED → fulfill grants from item.payload, mark APPROVED.
//!        DECLINED → mark DECLINED, return 402.
//!        PENDING  → store `redirectTo`, mark PENDING, return 202 + redirect.
//!   5. Persist the raw Etomin response on the payments row for audit.
//!
//! 3DS reconciliation: when Etomin replies PENDING the user must complete the
//! 3DS challenge at `redirectTo`. There's currently no public webhook/status
//! endpoint documented, so the row stays PENDING until manually reconciled
//! (TODO: wire webhook once docs are available).
use axum::{extract::State, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::extractors::Claims;
use crate::handlers::grants_util::apply_grant;
use crate::models::store_types::validate_grants;
use crate::services::etomin::{EtominError, SaleRequest};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ChargeRequest {
    pub item_id: Uuid,
    pub customer_information: serde_json::Value,
    pub card_data: serde_json::Value,
    /// URL Etomin redirects to after a 3DS challenge completes. Optional.
    pub redirect_url: Option<String>,
}

#[derive(Serialize)]
pub struct ChargeResponse {
    pub payment_id: Uuid,
    pub status: String,
    pub redirect_to: Option<String>,
    pub etomin_response: serde_json::Value,
}

/// Hardcoded ISO 4217 numeric currency code for now (484 = MXN).
const ETOMIN_CURRENCY: &str = "484";

pub async fn charge(
    State(state): State<AppState>,
    Claims(session): Claims,
    Json(payload): Json<ChargeRequest>,
) -> Result<(StatusCode, Json<ChargeResponse>), StatusCode> {
    // Validate item state first (404/410/400) — user-facing checks should
    // answer regardless of whether Etomin is configured.
    let row: Option<(i64, String, bool, serde_json::Value, String)> = sqlx::query_as(
        "SELECT cost, currency, is_active, payload, name FROM store_items WHERE id = $1",
    )
    .bind(payload.item_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let (cost, item_currency, is_active, item_payload, item_name) =
        row.ok_or(StatusCode::NOT_FOUND)?;
    if !is_active {
        return Err(StatusCode::GONE);
    }
    if item_currency != "iap" {
        return Err(StatusCode::BAD_REQUEST);
    }

    // After input validation, require the Etomin client.
    let etomin = state.etomin.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    // Insert payment row (PENDING). Its id becomes the Etomin `reference`.
    let (payment_id,): (Uuid,) = sqlx::query_as(
        r#"INSERT INTO payments (user_id, item_id, amount, currency, status)
           VALUES ($1, $2, $3, $4, 'PENDING') RETURNING id"#,
    )
    .bind(session.user_id)
    .bind(payload.item_id)
    .bind(cost)
    .bind(ETOMIN_CURRENCY)
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let reference = payment_id.to_string();
    let sale = SaleRequest {
        amount: cost,
        currency: ETOMIN_CURRENCY,
        reference: &reference,
        customer_information: payload.customer_information,
        card_data: payload.card_data,
        redirect_url: payload.redirect_url.as_deref(),
    };

    let etomin_response = match etomin.sale(&sale).await {
        Ok(v) => v,
        Err(EtominError::Auth) => {
            sqlx::query("UPDATE payments SET status='DECLINED', updated_at=CURRENT_TIMESTAMP WHERE id=$1")
                .bind(payment_id)
                .execute(&state.db)
                .await
                .ok();
            return Err(StatusCode::BAD_GATEWAY);
        }
        Err(EtominError::Upstream(_)) => {
            sqlx::query("UPDATE payments SET status='DECLINED', updated_at=CURRENT_TIMESTAMP WHERE id=$1")
                .bind(payment_id)
                .execute(&state.db)
                .await
                .ok();
            return Err(StatusCode::BAD_GATEWAY);
        }
        Err(EtominError::Local(_)) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let etomin_status = etomin_response
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("DECLINED")
        .to_uppercase();
    let redirect_to = etomin_response
        .get("redirectTo")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    // Etomin's internal id — needed later for status reconciliation.
    let etomin_id = etomin_response
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let (final_status, http_status) = match etomin_status.as_str() {
        "APPROVED" => ("APPROVED", StatusCode::OK),
        "PENDING" => ("PENDING", StatusCode::ACCEPTED),
        // Anything else (DECLINED, ERROR, blank) → DECLINED.
        _ => ("DECLINED", StatusCode::PAYMENT_REQUIRED),
    };

    // Persist Etomin's response and the resolved status. Use a transaction so
    // the grants application + final state update commit together.
    let mut tx = state.db.begin().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query(
        r#"UPDATE payments
           SET status = $2, redirect_to = $3, etomin_response = $4,
               etomin_id = $5, updated_at = CURRENT_TIMESTAMP
           WHERE id = $1"#,
    )
    .bind(payment_id)
    .bind(final_status)
    .bind(redirect_to.as_deref())
    .bind(&etomin_response)
    .bind(etomin_id.as_deref())
    .execute(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if final_status == "APPROVED" {
        // Fulfill the item's payload (an array of Grants) inside the same tx.
        let grants =
            validate_grants(&item_payload).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        for g in &grants {
            apply_grant(&mut tx, session.user_id, g, "iap_payment", payment_id).await?;
        }
    }

    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Fire-and-forget receipt for the frictionless-approval path. The 3DS
    // path approves later via the reconciler, which sends its own receipt.
    if final_status == "APPROVED" {
        crate::services::mailer::dispatch_purchase_receipt(
            &state,
            session.user_id,
            item_name,
            cost,
            "$".to_string(),
            payment_id.to_string(),
            "Tarjeta".to_string(),
        );
    }

    Ok((
        http_status,
        Json(ChargeResponse {
            payment_id,
            status: final_status.to_string(),
            redirect_to,
            etomin_response,
        }),
    ))
}
