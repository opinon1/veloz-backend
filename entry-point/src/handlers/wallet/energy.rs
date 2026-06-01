//! Energy regeneration (lazy).
//!
//! Spec:
//!   - 1 energy / minute, capped at 50.
//!   - Regen only runs while stored energy < 50. Above 50 (store
//!     purchases of refill packs), regen is paused; once the user spends
//!     energy back to < 50 the clock restarts from that moment.
//!
//! Implementation: `wallets.energy_refill_started_at` is a TIMESTAMPTZ
//! that marks "when does the next tick land". NULL = no clock running.
//!
//! Lazy: `refill_in_place` runs at the start of every read/spend/grant
//! against a `wallets` row. We compute floor(minutes elapsed since the
//! clock started), cap the new value at 50, persist, and advance the
//! clock by exactly (full_minutes * 60s) so partial minutes don't get
//! truncated away.
//!
//! Mutation: `reconcile_clock_for_energy` looks at the post-mutation
//! energy value and sets/clears the clock to match the < 50 invariant.

use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub const ENERGY_REGEN_CAP: i64 = 50;
pub const ENERGY_REGEN_SECS: i64 = 60;

/// Bring stored energy up to date for one user inside a transaction.
/// No-op if energy >= cap or the clock isn't running. Returns the
/// up-to-date (energy, clock) pair so the caller can echo it back to the
/// client without a second SELECT.
pub async fn refill_in_place(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> Result<(i64, Option<DateTime<Utc>>), StatusCode> {
    let row: Option<(i64, Option<DateTime<Utc>>)> = sqlx::query_as(
        "SELECT energy, energy_refill_started_at FROM wallets WHERE user_id = $1 FOR UPDATE",
    )
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (mut energy, mut clock) = row.ok_or(StatusCode::NOT_FOUND)?;

    // Start the clock lazily for any wallet with energy < cap and no
    // clock set. Covers brand-new users (the signup trigger doesn't
    // touch the column) without needing a separate one-shot fixup path.
    if energy < ENERGY_REGEN_CAP && clock.is_none() {
        let now = Utc::now();
        sqlx::query("UPDATE wallets SET energy_refill_started_at = $2 WHERE user_id = $1")
            .bind(user_id)
            .bind(now)
            .execute(&mut **tx)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        clock = Some(now);
    }

    let Some(started) = clock else {
        return Ok((energy, None));
    };

    if energy >= ENERGY_REGEN_CAP {
        // Defensive: cap reached but clock not cleared. Clear it.
        sqlx::query("UPDATE wallets SET energy_refill_started_at = NULL WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut **tx)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        return Ok((energy, None));
    }

    let now = Utc::now();
    let elapsed = (now - started).num_seconds();
    if elapsed < ENERGY_REGEN_SECS {
        return Ok((energy, Some(started)));
    }

    let full_minutes = elapsed / ENERGY_REGEN_SECS;
    let room = ENERGY_REGEN_CAP - energy;
    let granted = full_minutes.min(room);
    energy += granted;

    clock = if energy >= ENERGY_REGEN_CAP {
        None
    } else {
        // Advance the anchor by exactly `granted * 60s` so partial seconds
        // carry over into the next tick. (Not `now`, which would burn the
        // remainder.)
        Some(started + chrono::Duration::seconds(granted * ENERGY_REGEN_SECS))
    };

    sqlx::query(
        "UPDATE wallets SET energy = $2, energy_refill_started_at = $3, updated_at = CURRENT_TIMESTAMP WHERE user_id = $1",
    )
    .bind(user_id)
    .bind(energy)
    .bind(clock)
    .execute(&mut **tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Ledger: only if we actually granted something.
    if granted > 0 {
        sqlx::query(
            "INSERT INTO wallet_ledger (user_id, currency, delta, reason) VALUES ($1,'energy',$2,'regen')",
        )
        .bind(user_id)
        .bind(granted)
        .execute(&mut **tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    Ok((energy, clock))
}

/// After an energy mutation (spend/grant), make sure the clock matches
/// the post-state. Above cap => clock must be NULL. Below cap => clock
/// must be set; if already set, leave it alone (don't reset progress).
pub async fn reconcile_clock_for_energy(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> Result<(), StatusCode> {
    let row: Option<(i64, Option<DateTime<Utc>>)> = sqlx::query_as(
        "SELECT energy, energy_refill_started_at FROM wallets WHERE user_id = $1 FOR UPDATE",
    )
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (energy, clock) = row.ok_or(StatusCode::NOT_FOUND)?;

    match (energy >= ENERGY_REGEN_CAP, clock.is_some()) {
        (true, true) => {
            sqlx::query("UPDATE wallets SET energy_refill_started_at = NULL WHERE user_id = $1")
                .bind(user_id)
                .execute(&mut **tx)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        (false, false) => {
            sqlx::query(
                "UPDATE wallets SET energy_refill_started_at = CURRENT_TIMESTAMP WHERE user_id = $1",
            )
            .bind(user_id)
            .execute(&mut **tx)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        _ => {}
    }
    Ok(())
}
