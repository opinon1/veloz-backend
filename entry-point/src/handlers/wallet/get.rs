use super::energy::refill_in_place;
use crate::extractors::Claims;
use crate::state::AppState;
use axum::{Json, extract::State, http::StatusCode};
use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Serialize)]
pub struct WalletResponse {
    pub high: i64,
    pub soft: i64,
    pub energy: i64,
    /// When the next energy tick will land. Null when stored energy is
    /// at or above the regen cap (50) — clock is paused.
    pub energy_refill_started_at: Option<DateTime<Utc>>,
}

pub async fn get_wallet(
    State(state): State<AppState>,
    Claims(session): Claims,
) -> Result<Json<WalletResponse>, StatusCode> {
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Lazy energy regen: catches up the stored energy before we read it.
    let (energy, clock) = refill_in_place(&mut tx, session.user_id).await?;

    let (high, soft): (i64, i64) =
        sqlx::query_as("SELECT high, soft FROM wallets WHERE user_id = $1")
            .bind(session.user_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(WalletResponse {
        high,
        soft,
        energy,
        energy_refill_started_at: clock,
    }))
}
