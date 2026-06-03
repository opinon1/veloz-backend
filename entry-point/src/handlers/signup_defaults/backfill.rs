//! POST /admin/signup-defaults/backfill — applies the current default
//! catalog rows to every existing user. Idempotent on each user via
//! ON CONFLICT DO NOTHING + the `default_grants_applied` table for
//! store payloads.

use axum::{Json, extract::State, http::StatusCode};
use serde::Serialize;
use uuid::Uuid;

use crate::extractors::AdminClaims;
use crate::state::AppState;

use super::service::{DefaultsApplied, apply_defaults_for_user};

#[derive(Serialize)]
pub struct BackfillResponse {
    pub users_processed: u32,
    pub totals: DefaultsApplied,
}

pub async fn backfill(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
) -> Result<Json<BackfillResponse>, StatusCode> {
    let user_ids: Vec<(Uuid,)> = sqlx::query_as("SELECT id FROM users")
        .fetch_all(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut totals = DefaultsApplied::default();
    let mut processed: u32 = 0;
    // One small transaction per user so a single bad payload row
    // doesn't take down the entire backfill — committed work for
    // earlier users sticks.
    for (uid,) in user_ids {
        let mut tx = state
            .db
            .begin()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        match apply_defaults_for_user(&mut tx, uid).await {
            Ok(t) => {
                tx.commit()
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                totals += t;
                processed += 1;
            }
            Err(_) => {
                // Roll back this user's tx and keep going.
                let _ = tx.rollback().await;
            }
        }
    }

    Ok(Json(BackfillResponse {
        users_processed: processed,
        totals,
    }))
}
