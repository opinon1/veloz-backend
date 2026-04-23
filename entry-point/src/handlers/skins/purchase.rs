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

    // Check ownership first to avoid double-charging.
    let owned: Option<(Uuid,)> = sqlx::query_as(
        "SELECT user_id FROM user_skins WHERE user_id = $1 AND skin_id = $2",
    )
    .bind(session.user_id)
    .bind(skin_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if owned.is_some() {
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
        // Free skin — no ledger entry.
        0
    };

    sqlx::query(
        "INSERT INTO user_skins (user_id, skin_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(session.user_id)
    .bind(skin_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(PurchaseSkinResponse {
        skin_id,
        currency,
        cost_paid: cost,
        new_balance,
    }))
}
