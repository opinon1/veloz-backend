use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(sqlx::FromRow)]
pub struct ActiveSeason {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub premium_cost: i64,
    pub premium_currency: String,
}

/// Returns the currently active season, or None if no season is running.
pub async fn active_season(pool: &PgPool) -> Result<Option<ActiveSeason>, StatusCode> {
    sqlx::query_as::<_, ActiveSeason>(
        r#"
        SELECT id, name, description, starts_at, ends_at, premium_cost, premium_currency
        FROM bp_seasons
        WHERE starts_at <= CURRENT_TIMESTAMP AND ends_at > CURRENT_TIMESTAMP
        ORDER BY starts_at DESC
        LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}
