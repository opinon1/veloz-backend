use axum::{extract::{Path, State}, Json, http::StatusCode};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::Claims;

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
pub async fn get_payment(
    State(state): State<AppState>,
    Claims(session): Claims,
    Path(id): Path<Uuid>,
) -> Result<Json<PaymentRow>, StatusCode> {
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
