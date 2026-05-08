//! Shared Grant fulfillment.
//!
//! `Grant`s appear in store payloads, battlepass tier rewards, and prize-wheel
//! items. Anywhere a feature needs to *apply* a grant — credit currency,
//! unlock a skin — it should call `apply_grant` so the per-variant logic
//! lives in exactly one place. Adding a new Grant variant produces a compile
//! error here, forcing every fulfillment site to handle it.
use axum::http::StatusCode;
use uuid::Uuid;

use crate::handlers::wallet::utils::adjust_balance;
use crate::models::store_types::Grant;

/// Apply a single Grant inside the caller's transaction.
///
/// `reason` is the audit string written to the wallet ledger for currency
/// grants (e.g. `"prize_wheel_spin"`, `"store_grant"`). `reference_id` is the
/// originating row id (item id, spin id, etc.) so the ledger row points back
/// at what caused the grant.
pub async fn apply_grant(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: Uuid,
    grant: &Grant,
    reason: &str,
    reference_id: Uuid,
) -> Result<(), StatusCode> {
    match grant {
        Grant::Currency { currency, amount } => {
            adjust_balance(
                tx,
                user_id,
                currency.as_str(),
                *amount,
                reason,
                Some(&reference_id.to_string()),
            )
            .await?;
        }
        Grant::Skin { skin_id } => {
            sqlx::query(
                "INSERT INTO user_skins (user_id, skin_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(user_id)
            .bind(skin_id)
            .execute(&mut **tx)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }
    Ok(())
}
