use axum::{extract::{Path, State}, Json, http::StatusCode};
use serde::Serialize;
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::Claims;
use crate::handlers::wallet::utils::adjust_balance;

#[derive(Serialize)]
pub struct PurchaseFrameResponse {
    pub frame_id: Uuid,
    pub currency: String,
    pub cost_paid: i64,
    pub new_balance: i64,
}

pub async fn purchase_frame(
    State(state): State<AppState>,
    Claims(session): Claims,
    Path(frame_id): Path<Uuid>,
) -> Result<Json<PurchaseFrameResponse>, StatusCode> {
    let mut tx = state.db.begin().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let row: Option<(i64, String, bool)> = sqlx::query_as(
        "SELECT price, currency, is_active FROM frames WHERE id = $1",
    )
    .bind(frame_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (cost, currency, is_active) = row.ok_or(StatusCode::NOT_FOUND)?;
    if !is_active {
        return Err(StatusCode::GONE);
    }

    let inserted = sqlx::query(
        "INSERT INTO user_frames (user_id, frame_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(session.user_id)
    .bind(frame_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if inserted.rows_affected() == 0 {
        return Err(StatusCode::CONFLICT);
    }

    let new_balance = if cost > 0 {
        adjust_balance(
            &mut tx,
            session.user_id,
            &currency,
            -cost,
            "frame_purchase",
            Some(&frame_id.to_string()),
        )
        .await?
    } else {
        let q = format!("SELECT {col} FROM wallets WHERE user_id = $1", col = currency);
        let (bal,): (i64,) = sqlx::query_as(&q)
            .bind(session.user_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        bal
    };

    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(PurchaseFrameResponse {
        frame_id,
        currency,
        cost_paid: cost,
        new_balance,
    }))
}
