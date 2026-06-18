use axum::{extract::{Path, State}, Json, http::StatusCode};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::Claims;
use super::reconcile::reconcile_payment;

#[derive(Serialize, sqlx::FromRow)]
pub struct PaymentRow {
    pub id: Uuid,
    pub item_id: Uuid,
    pub amount: i64,
    pub currency: String,
    pub status: String,
    pub redirect_to: Option<String>,
    pub etomin_response: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// GET /payments/{id} — only returns rows owned by the caller.
///
/// Side effect: when the row is still PENDING, kicks off a reconcile against
/// Etomin's status endpoint before returning. This is how we converge to
/// terminal state when the user closed the 3DS tab and never came back —
/// any subsequent poll re-queries Etomin and applies grants if APPROVED.
pub async fn get_payment(
    State(state): State<AppState>,
    Claims(session): Claims,
    Path(id): Path<Uuid>,
) -> Result<Json<PaymentRow>, StatusCode> {
    // Pre-check: row must exist and be the caller's. Avoids running
    // reconcile for someone else's payment (and leaking that it exists).
    let exists: Option<(String,)> =
        sqlx::query_as("SELECT status FROM payments WHERE id = $1 AND user_id = $2")
            .bind(id)
            .bind(session.user_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let (status,) = exists.ok_or(StatusCode::NOT_FOUND)?;

    if status == "PENDING" {
        if let Some(etomin) = state.etomin.as_ref() {
            // Best-effort. Etomin / network failures don't block the read —
            // caller still sees the cached PENDING row, sweeper retries.
            let _ = reconcile_payment(&state.db, etomin, state.mailer.as_ref(), id).await;
        }
    }

    let row = sqlx::query_as::<_, PaymentRow>(
        r#"SELECT id, item_id, amount, currency, status, redirect_to, etomin_response,
                  created_at, updated_at
           FROM payments WHERE id = $1 AND user_id = $2"#,
    )
    .bind(id)
    .bind(session.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(row))
}
