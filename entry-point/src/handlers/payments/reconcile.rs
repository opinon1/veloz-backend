//! Reconcile a PENDING payment with Etomin's authoritative state.
//!
//! Used by both:
//!   - `GET /payments/{id}` — lazy reconcile when the user polls
//!   - background sweeper task in `main.rs` — catches abandoned tabs
//!
//! No-op fast paths:
//!   - row not found
//!   - row.status already terminal (APPROVED / DECLINED / EXPIRED)
//!   - row.etomin_id is NULL (initial /sale never returned an id)
//!
//! Trust model: Etomin's HTTP response is authoritative. We never trust the
//! user-redirect query string. If Etomin says APPROVED, we apply the item's
//! grant payload inside the same DB transaction that flips the status — so
//! a crash in the middle leaves us in a recoverable state (still PENDING,
//! next reconcile retries).
use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::handlers::grants_util::apply_grant;
use crate::models::store_types::validate_grants;
use crate::services::etomin::EtominClient;

/// PENDING rows older than this are marked EXPIRED locally without bothering
/// Etomin (their 3DS challenge typically expires in 15-30 min). 1h is a safe
/// upper bound that won't EXPIRE legitimate slow flows.
const EXPIRY_HOURS: i64 = 1;

#[derive(sqlx::FromRow)]
struct PaymentForReconcile {
    id: Uuid,
    user_id: Uuid,
    item_id: Uuid,
    status: String,
    etomin_id: Option<String>,
    created_at: DateTime<Utc>,
}

/// Pull the latest state from Etomin and update our row + apply grants if
/// the new state is APPROVED. Returns `Ok(true)` if the row's status
/// changed (so callers can decide whether to refetch and return fresh data).
pub async fn reconcile_payment(
    db: &PgPool,
    etomin: &EtominClient,
    payment_id: Uuid,
) -> Result<bool, ()> {
    let row: Option<PaymentForReconcile> = sqlx::query_as(
        "SELECT id, user_id, item_id, status, etomin_id, created_at FROM payments WHERE id = $1",
    )
    .bind(payment_id)
    .fetch_optional(db)
    .await
    .map_err(|_| ())?;
    let row = match row {
        Some(r) => r,
        None => return Ok(false),
    };

    if row.status != "PENDING" {
        return Ok(false);
    }

    // Hard timeout: PENDING longer than EXPIRY_HOURS is dead. Don't poll
    // Etomin — just mark EXPIRED and move on.
    if Utc::now() - row.created_at > Duration::hours(EXPIRY_HOURS) {
        sqlx::query(
            "UPDATE payments SET status='EXPIRED', updated_at=CURRENT_TIMESTAMP WHERE id=$1 AND status='PENDING'",
        )
        .bind(row.id)
        .execute(db)
        .await
        .map_err(|_| ())?;
        return Ok(true);
    }

    let etomin_id = match row.etomin_id.as_deref() {
        Some(s) => s,
        None => return Ok(false),
    };

    let resp = match etomin.transaction_status(etomin_id).await {
        Ok(v) => v,
        // Transient errors should NOT terminate the row — return Ok(false)
        // so the next sweep retries.
        Err(_) => return Ok(false),
    };

    let new_status = resp
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("PENDING")
        .to_uppercase();

    match new_status.as_str() {
        "APPROVED" => {
            apply_approval(db, &row, &resp).await?;
            Ok(true)
        }
        "DECLINED" => {
            sqlx::query(
                "UPDATE payments SET status='DECLINED', etomin_response=$2, updated_at=CURRENT_TIMESTAMP WHERE id=$1 AND status='PENDING'",
            )
            .bind(row.id)
            .bind(&resp)
            .execute(db)
            .await
            .map_err(|_| ())?;
            Ok(true)
        }
        _ => {
            // Still pending — touch updated_at so the sweeper's `ORDER BY`
            // gives newer probes lower priority.
            sqlx::query(
                "UPDATE payments SET etomin_response=$2, updated_at=CURRENT_TIMESTAMP WHERE id=$1 AND status='PENDING'",
            )
            .bind(row.id)
            .bind(&resp)
            .execute(db)
            .await
            .map_err(|_| ())?;
            Ok(false)
        }
    }
}

async fn apply_approval(
    db: &PgPool,
    row: &PaymentForReconcile,
    etomin_response: &serde_json::Value,
) -> Result<(), ()> {
    let item_payload: Option<(serde_json::Value,)> =
        sqlx::query_as("SELECT payload FROM store_items WHERE id = $1")
            .bind(row.item_id)
            .fetch_optional(db)
            .await
            .map_err(|_| ())?;
    let payload = match item_payload {
        Some((p,)) => p,
        None => return Err(()),
    };
    let grants = validate_grants(&payload).map_err(|_| ())?;

    let mut tx = db.begin().await.map_err(|_| ())?;

    // Compare-and-swap: only flip if still PENDING. Stops two concurrent
    // reconciles (lazy + sweeper) from double-applying grants.
    let updated = sqlx::query(
        "UPDATE payments SET status='APPROVED', etomin_response=$2, updated_at=CURRENT_TIMESTAMP WHERE id=$1 AND status='PENDING'",
    )
    .bind(row.id)
    .bind(etomin_response)
    .execute(&mut *tx)
    .await
    .map_err(|_| ())?;

    if updated.rows_affected() == 0 {
        // Someone else won the race. Their grants applied (or will). Bail.
        tx.rollback().await.ok();
        return Ok(());
    }

    for g in &grants {
        let _ = apply_grant(&mut tx, row.user_id, g, "iap_payment", row.id).await;
    }

    tx.commit().await.map_err(|_| ())?;
    Ok(())
}

/// One sweep of the background reconciler. Caps the number of rows touched
/// per sweep so a sudden backlog doesn't burn through Etomin rate limits.
pub async fn sweep(db: &PgPool, etomin: &EtominClient, limit: i64) {
    // Two-step: pick stuck rows, then reconcile each. Doing them serially
    // avoids hammering Etomin in parallel; if throughput becomes a problem
    // we can `tokio::join!` a small set later.
    let pending: Vec<(Uuid,)> = match sqlx::query_as(
        "SELECT id FROM payments WHERE status='PENDING' ORDER BY updated_at ASC LIMIT $1",
    )
    .bind(limit)
    .fetch_all(db)
    .await
    {
        Ok(v) => v,
        Err(_) => return,
    };

    for (id,) in pending {
        let _ = reconcile_payment(db, etomin, id).await;
    }
}
