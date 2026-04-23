use axum::{extract::{Path, State}, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::AdminClaims;

#[derive(Deserialize)]
pub struct CreateSkinRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub outfit_url: String,
    #[serde(default)]
    pub cost: i64,
    #[serde(default = "default_currency")]
    pub currency: String,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub metadata: serde_json::Value,
}
fn default_currency() -> String { "soft".into() }

#[derive(Serialize, sqlx::FromRow)]
pub struct SkinRow {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub outfit_url: String,
    pub cost: i64,
    pub currency: String,
    pub is_default: bool,
    pub is_active: bool,
    pub metadata: serde_json::Value,
}

pub async fn create_skin(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Json(payload): Json<CreateSkinRequest>,
) -> Result<(StatusCode, Json<SkinRow>), StatusCode> {
    let row = sqlx::query_as::<_, SkinRow>(
        r#"
        INSERT INTO skins (name, description, outfit_url, cost, currency, is_default, metadata)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id, name, description, outfit_url, cost, currency, is_default, is_active, metadata
        "#,
    )
    .bind(&payload.name)
    .bind(&payload.description)
    .bind(&payload.outfit_url)
    .bind(payload.cost)
    .bind(&payload.currency)
    .bind(payload.is_default)
    .bind(&payload.metadata)
    .fetch_one(&state.db)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(db) if db.code().as_deref() == Some("23505") => StatusCode::CONFLICT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    })?;

    Ok((StatusCode::CREATED, Json(row)))
}

pub async fn list_all_skins(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
) -> Result<Json<Vec<SkinRow>>, StatusCode> {
    let rows = sqlx::query_as::<_, SkinRow>(
        "SELECT id, name, description, outfit_url, cost, currency, is_default, is_active, metadata FROM skins ORDER BY created_at DESC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct UpdateSkinRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub outfit_url: Option<String>,
    pub cost: Option<i64>,
    pub currency: Option<String>,
    pub is_default: Option<bool>,
    pub is_active: Option<bool>,
    pub metadata: Option<serde_json::Value>,
}

pub async fn update_skin(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateSkinRequest>,
) -> Result<Json<SkinRow>, StatusCode> {
    let row = sqlx::query_as::<_, SkinRow>(
        r#"
        UPDATE skins SET
            name         = COALESCE($2, name),
            description  = COALESCE($3, description),
            outfit_url   = COALESCE($4, outfit_url),
            cost         = COALESCE($5, cost),
            currency     = COALESCE($6, currency),
            is_default   = COALESCE($7, is_default),
            is_active    = COALESCE($8, is_active),
            metadata     = COALESCE($9, metadata),
            updated_at   = CURRENT_TIMESTAMP
        WHERE id = $1
        RETURNING id, name, description, outfit_url, cost, currency, is_default, is_active, metadata
        "#,
    )
    .bind(id)
    .bind(&payload.name)
    .bind(&payload.description)
    .bind(&payload.outfit_url)
    .bind(payload.cost)
    .bind(&payload.currency)
    .bind(payload.is_default)
    .bind(payload.is_active)
    .bind(&payload.metadata)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(row))
}

pub async fn delete_skin(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let result = sqlx::query("DELETE FROM skins WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}
