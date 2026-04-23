use axum::{extract::{Path, State}, Json, http::StatusCode};
use serde::Serialize;
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::Claims;
use crate::handlers::wallet::utils::adjust_balance;

#[derive(Serialize)]
pub struct PurchaseSkinResponse {
    pub skin_id: Uuid,
    pub currency: String,
    pub cost_paid: i64,
    pub new_balance: i64,
}

pub async fn purchase_skin(
    State(state): State<AppState>,
    Claims(session): Claims,
    Path(skin_id): Path<Uuid>,
) -> Result<Json<PurchaseSkinResponse>, StatusCode> {
    let mut tx = state.db.begin().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let skin: Option<(i64, String, bool)> = sqlx::query_as(
        "SELECT cost, currency, is_active FROM skins WHERE id = $1",
    )
    .bind(skin_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (cost, currency, is_active) = skin.ok_or(StatusCode::NOT_FOUND)?;
    if !is_active {
        return Err(StatusCode::GONE);
    }

    // Claim ownership FIRST. ON CONFLICT DO NOTHING returns 0 rows_affected
    // when the user already owns the skin — treat that as 409 without
    // charging. Doing this before the wallet deduction closes the
    // check-then-act race where two concurrent requests could both pass an
    // "is it owned?" SELECT and double-charge.
    let inserted = sqlx::query(
        "INSERT INTO user_skins (user_id, skin_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(session.user_id)
    .bind(skin_id)
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
            "skin_purchase",
            Some(&skin_id.to_string()),
        )
        .await?
    } else {
        // Free skin — no ledger entry. Still return actual current balance
        // so clients display accurate UI.
        let q = format!("SELECT {col} FROM wallets WHERE user_id = $1", col = currency);
        let (bal,): (i64,) = sqlx::query_as(&q)
            .bind(session.user_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        bal
    };

    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(PurchaseSkinResponse {
        skin_id,
        currency,
        cost_paid: cost,
        new_balance,
    }))
}
