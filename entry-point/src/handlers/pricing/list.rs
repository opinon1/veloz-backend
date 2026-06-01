//! GET /me/prices — every per-user dynamic price the client might
//! want to display, in one round-trip.
//!
//! Each entry exposes both `base_cost` (the catalog cost the admin
//! set) and `cost_for_you` (after the per-user dynamic curve + the
//! account-wide `price_multiplier`). Frontend joins by `(kind, id)`.
//!
//! IAP-priced store items are skipped — real-money tiers are governed
//! by store / Apple / Google contracts, not by our dynamic curve.

use axum::{Json, extract::State, http::StatusCode};
use serde::Serialize;
use uuid::Uuid;

use crate::extractors::Claims;
use crate::pricing::apply_dynamic_price;
use crate::state::AppState;

#[derive(Serialize)]
pub struct PriceRow {
    /// One of: `store`, `skin`, `avatar`, `frame`. Matches the
    /// endpoint that sells the row.
    pub kind: &'static str,
    pub id: Uuid,
    pub currency: String,
    pub base_cost: i64,
    pub cost_for_you: i64,
}

#[derive(sqlx::FromRow)]
struct CatalogRow {
    id: Uuid,
    cost: i64,
    currency: String,
}

pub async fn list_my_prices(
    State(state): State<AppState>,
    Claims(session): Claims,
) -> Result<Json<Vec<PriceRow>>, StatusCode> {
    // One read for the user's progression + flat account discount.
    let (total_xp, account_multiplier): (i64, f64) = sqlx::query_as(
        "SELECT total_xp, price_multiplier FROM profiles WHERE user_id = $1",
    )
    .bind(session.user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Each catalog lives in its own table with a slightly different
    // column name for the price (`cost` vs `price`). Normalize them
    // to a common (id, cost, currency) shape.
    let store: Vec<CatalogRow> = sqlx::query_as(
        "SELECT id, cost, currency FROM store_items WHERE is_active = TRUE AND currency <> 'iap'",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let skins: Vec<CatalogRow> = sqlx::query_as(
        "SELECT id, cost, currency FROM skins WHERE is_active = TRUE",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let avatars: Vec<CatalogRow> = sqlx::query_as(
        "SELECT id, price AS cost, currency FROM avatars WHERE is_active = TRUE",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let frames: Vec<CatalogRow> = sqlx::query_as(
        "SELECT id, price AS cost, currency FROM frames WHERE is_active = TRUE",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut out: Vec<PriceRow> =
        Vec::with_capacity(store.len() + skins.len() + avatars.len() + frames.len());

    let push = |kind: &'static str, rows: Vec<CatalogRow>, out: &mut Vec<PriceRow>| {
        for r in rows {
            let cost_for_you = apply_dynamic_price(
                r.cost,
                session.user_id,
                r.id,
                total_xp,
                account_multiplier,
            );
            out.push(PriceRow {
                kind,
                id: r.id,
                currency: r.currency,
                base_cost: r.cost,
                cost_for_you,
            });
        }
    };
    push("store", store, &mut out);
    push("skin", skins, &mut out);
    push("avatar", avatars, &mut out);
    push("frame", frames, &mut out);

    Ok(Json(out))
}
