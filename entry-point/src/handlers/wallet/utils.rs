use axum::http::StatusCode;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

/// Valid currency codes. Update here + SQL CHECKs when new currencies are added.
pub fn is_valid_currency(currency: &str) -> bool {
    matches!(currency, "high" | "soft" | "energy")
}

/// Apply a delta to a user's wallet atomically. Positive = grant, negative = spend.
/// Returns the new balance of the updated currency. Errors with UNPROCESSABLE_ENTITY
/// if the delta would leave the balance negative (CHECK constraint violation).
pub async fn adjust_balance(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    currency: &str,
    delta: i64,
    reason: &str,
    reference_id: Option<&str>,
) -> Result<i64, StatusCode> {
    if !is_valid_currency(currency) {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Column names are validated above — safe to interpolate into the UPDATE.
    let query = format!(
        "UPDATE wallets SET {col} = {col} + $2, updated_at = CURRENT_TIMESTAMP WHERE user_id = $1 RETURNING {col}",
        col = currency
    );

    let new_balance: (i64,) = sqlx::query_as(&query)
        .bind(user_id)
        .bind(delta)
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(db) if db.code().as_deref() == Some("23514") => {
                StatusCode::UNPROCESSABLE_ENTITY // check constraint: insufficient funds
            }
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        })?;

    sqlx::query(
        r#"INSERT INTO wallet_ledger (user_id, currency, delta, reason, reference_id)
           VALUES ($1, $2, $3, $4, $5)"#,
    )
    .bind(user_id)
    .bind(currency)
    .bind(delta)
    .bind(reason)
    .bind(reference_id)
    .execute(&mut **tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(new_balance.0)
}

/// Helper: adjust balance with its own transaction (for single-shot spends/grants).
pub async fn adjust_balance_oneshot(
    pool: &PgPool,
    user_id: Uuid,
    currency: &str,
    delta: i64,
    reason: &str,
    reference_id: Option<&str>,
) -> Result<i64, StatusCode> {
    let mut tx = pool.begin().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let new_balance = adjust_balance(&mut tx, user_id, currency, delta, reason, reference_id).await?;
    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(new_balance)
}
