use axum::{extract::{Path, State}, Json, http::StatusCode};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::AdminClaims;

// ─────────── Seasons ───────────

#[derive(Deserialize)]
pub struct CreateSeasonRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    #[serde(default)]
    pub premium_cost: i64,
    #[serde(default = "default_high")]
    pub premium_currency: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}
fn default_high() -> String { "high".into() }

#[derive(Serialize, sqlx::FromRow)]
pub struct SeasonRow {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub premium_cost: i64,
    pub premium_currency: String,
    pub metadata: serde_json::Value,
}

pub async fn create_season(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Json(payload): Json<CreateSeasonRequest>,
) -> Result<(StatusCode, Json<SeasonRow>), StatusCode> {
    let row = sqlx::query_as::<_, SeasonRow>(
        r#"
        INSERT INTO bp_seasons (name, description, starts_at, ends_at, premium_cost, premium_currency, metadata)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id, name, description, starts_at, ends_at, premium_cost, premium_currency, metadata
        "#,
    )
    .bind(&payload.name)
    .bind(&payload.description)
    .bind(payload.starts_at)
    .bind(payload.ends_at)
    .bind(payload.premium_cost)
    .bind(&payload.premium_currency)
    .bind(&payload.metadata)
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((StatusCode::CREATED, Json(row)))
}

pub async fn list_seasons(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
) -> Result<Json<Vec<SeasonRow>>, StatusCode> {
    let rows = sqlx::query_as::<_, SeasonRow>(
        "SELECT id, name, description, starts_at, ends_at, premium_cost, premium_currency, metadata FROM bp_seasons ORDER BY starts_at DESC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct UpdateSeasonRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub premium_cost: Option<i64>,
    pub premium_currency: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

pub async fn update_season(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateSeasonRequest>,
) -> Result<Json<SeasonRow>, StatusCode> {
    let row = sqlx::query_as::<_, SeasonRow>(
        r#"
        UPDATE bp_seasons SET
            name             = COALESCE($2, name),
            description      = COALESCE($3, description),
            starts_at        = COALESCE($4, starts_at),
            ends_at          = COALESCE($5, ends_at),
            premium_cost     = COALESCE($6, premium_cost),
            premium_currency = COALESCE($7, premium_currency),
            metadata         = COALESCE($8, metadata)
        WHERE id = $1
        RETURNING id, name, description, starts_at, ends_at, premium_cost, premium_currency, metadata
        "#,
    )
    .bind(id)
    .bind(&payload.name)
    .bind(&payload.description)
    .bind(payload.starts_at)
    .bind(payload.ends_at)
    .bind(payload.premium_cost)
    .bind(&payload.premium_currency)
    .bind(&payload.metadata)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(row))
}

pub async fn delete_season(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let result = sqlx::query("DELETE FROM bp_seasons WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}

// ─────────── Tiers ───────────

#[derive(Deserialize)]
pub struct CreateTierRequest {
    pub tier: i32,
    pub xp_required: i64,
    #[serde(default)]
    pub free_reward: serde_json::Value,
    #[serde(default)]
    pub premium_reward: serde_json::Value,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct TierRow {
    pub id: Uuid,
    pub season_id: Uuid,
    pub tier: i32,
    pub xp_required: i64,
    pub free_reward: serde_json::Value,
    pub premium_reward: serde_json::Value,
}

pub async fn create_tier(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Path(season_id): Path<Uuid>,
    Json(payload): Json<CreateTierRequest>,
) -> Result<(StatusCode, Json<TierRow>), StatusCode> {
    let row = sqlx::query_as::<_, TierRow>(
        r#"
        INSERT INTO bp_tiers (season_id, tier, xp_required, free_reward, premium_reward)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, season_id, tier, xp_required, free_reward, premium_reward
        "#,
    )
    .bind(season_id)
    .bind(payload.tier)
    .bind(payload.xp_required)
    .bind(&payload.free_reward)
    .bind(&payload.premium_reward)
    .fetch_one(&state.db)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(db) if db.code().as_deref() == Some("23505") => StatusCode::CONFLICT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    })?;

    Ok((StatusCode::CREATED, Json(row)))
}

pub async fn list_tiers(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Path(season_id): Path<Uuid>,
) -> Result<Json<Vec<TierRow>>, StatusCode> {
    let rows = sqlx::query_as::<_, TierRow>(
        "SELECT id, season_id, tier, xp_required, free_reward, premium_reward FROM bp_tiers WHERE season_id = $1 ORDER BY tier ASC",
    )
    .bind(season_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct UpdateTierRequest {
    pub tier: Option<i32>,
    pub xp_required: Option<i64>,
    pub free_reward: Option<serde_json::Value>,
    pub premium_reward: Option<serde_json::Value>,
}

pub async fn update_tier(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateTierRequest>,
) -> Result<Json<TierRow>, StatusCode> {
    let row = sqlx::query_as::<_, TierRow>(
        r#"
        UPDATE bp_tiers SET
            tier           = COALESCE($2, tier),
            xp_required    = COALESCE($3, xp_required),
            free_reward    = COALESCE($4, free_reward),
            premium_reward = COALESCE($5, premium_reward)
        WHERE id = $1
        RETURNING id, season_id, tier, xp_required, free_reward, premium_reward
        "#,
    )
    .bind(id)
    .bind(payload.tier)
    .bind(payload.xp_required)
    .bind(&payload.free_reward)
    .bind(&payload.premium_reward)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(row))
}

pub async fn delete_tier(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let result = sqlx::query("DELETE FROM bp_tiers WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}
