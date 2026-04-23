use axum::{extract::State, Json, http::StatusCode};
use serde::Serialize;
use crate::state::AppState;
use crate::extractors::Claims;

#[derive(Serialize, sqlx::FromRow)]
pub struct WalletResponse {
    pub high: i64,
    pub soft: i64,
    pub energy: i64,
}

pub async fn get_wallet(
    State(state): State<AppState>,
    Claims(session): Claims,
) -> Result<Json<WalletResponse>, StatusCode> {
    let row = sqlx::query_as::<_, WalletResponse>(
        "SELECT high, soft, energy FROM wallets WHERE user_id = $1",
    )
    .bind(session.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(row))
}
