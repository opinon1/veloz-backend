use axum::{extract::State, Json, http::StatusCode};
use serde::Serialize;
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::Claims;
use crate::handlers::wallet::utils::adjust_balance;
use super::utils::active_season;

#[derive(Serialize)]
pub struct UnlockResponse {
    pub season_id: Uuid,
    pub currency: String,
    pub cost_paid: i64,
    pub new_balance: i64,
}

pub async fn unlock_premium(
    State(state): State<AppState>,
    Claims(session): Claims,
) -> Result<Json<UnlockResponse>, StatusCode> {
    let season = active_season(&state.db).await?.ok_or(StatusCode::NOT_FOUND)?;

    let mut tx = state.db.begin().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Ensure progress row exists.
    sqlx::query(
        r#"INSERT INTO bp_progress (user_id, season_id)
           VALUES ($1, $2)
           ON CONFLICT (user_id, season_id) DO NOTHING"#,
    )
    .bind(session.user_id)
    .bind(season.id)
    .execute(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Atomic claim: flip premium_unlocked from FALSE → TRUE. If the row was
    // already TRUE, rows_affected == 0 and we return 409 without charging.
    // This closes the race where two concurrent unlock calls could both pass
    // a SELECT premium_unlocked=FALSE check and both charge the wallet.
    let claim = sqlx::query(
        r#"UPDATE bp_progress
           SET premium_unlocked = TRUE, updated_at = CURRENT_TIMESTAMP
           WHERE user_id = $1 AND season_id = $2 AND premium_unlocked = FALSE"#,
    )
    .bind(session.user_id)
    .bind(season.id)
    .execute(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if claim.rows_affected() == 0 {
        return Err(StatusCode::CONFLICT);
    }

    // Charge inside the same tx — if adjust_balance errors (insufficient
    // funds), the rollback reverts the premium_unlocked flip as well.
    let new_balance = if season.premium_cost > 0 {
        adjust_balance(
            &mut tx,
            session.user_id,
            &season.premium_currency,
            -season.premium_cost,
            "bp_unlock",
            Some(&season.id.to_string()),
        )
        .await?
    } else {
        0
    };

    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(UnlockResponse {
        season_id: season.id,
        currency: season.premium_currency,
        cost_paid: season.premium_cost,
        new_balance,
    }))
}
