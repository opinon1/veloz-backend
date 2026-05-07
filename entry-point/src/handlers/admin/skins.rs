use axum::{extract::{Path, State}, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::AdminClaims;

#[derive(Deserialize)]
pub struct CreateSkinRequest {
    pub character_id: Uuid,
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
    pub character_id: Uuid,
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
    if !matches!(payload.currency.as_str(), "high" | "soft" | "energy") {
        return Err(StatusCode::BAD_REQUEST);
    }
    if payload.cost < 0 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let row = sqlx::query_as::<_, SkinRow>(
        r#"
        INSERT INTO skins (character_id, cost, currency, is_default, metadata)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, character_id, cost, currency, is_default, is_active, metadata
        "#,
    )
    .bind(payload.character_id)
    .bind(payload.cost)
    .bind(&payload.currency)
    .bind(payload.is_default)
    .bind(&payload.metadata)
    .fetch_one(&state.db)
    .await
    .map_err(|e| match e {
        // FK violation on character_id → 400 (caller passed an unknown char).
        sqlx::Error::Database(db) if db.code().as_deref() == Some("23503") => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    })?;

    Ok((StatusCode::CREATED, Json(row)))
}

pub async fn list_all_skins(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
) -> Result<Json<Vec<SkinRow>>, StatusCode> {
    let rows = sqlx::query_as::<_, SkinRow>(
        "SELECT id, character_id, cost, currency, is_default, is_active, metadata FROM skins ORDER BY created_at DESC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct UpdateSkinRequest {
    pub character_id: Option<Uuid>,
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
    if let Some(c) = payload.cost {
        if c < 0 {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    if let Some(ref cur) = payload.currency {
        if !matches!(cur.as_str(), "high" | "soft" | "energy") {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    let row = sqlx::query_as::<_, SkinRow>(
        r#"
        UPDATE skins SET
            character_id = COALESCE($2, character_id),
            cost         = COALESCE($3, cost),
            currency     = COALESCE($4, currency),
            is_default   = COALESCE($5, is_default),
            is_active    = COALESCE($6, is_active),
            metadata     = COALESCE($7, metadata),
            updated_at   = CURRENT_TIMESTAMP
        WHERE id = $1
        RETURNING id, character_id, cost, currency, is_default, is_active, metadata
        "#,
    )
    .bind(id)
    .bind(payload.character_id)
    .bind(payload.cost)
    .bind(&payload.currency)
    .bind(payload.is_default)
    .bind(payload.is_active)
    .bind(&payload.metadata)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(db) if db.code().as_deref() == Some("23503") => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(row))
}

pub async fn delete_skin(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    // user_skins cascades on FK; user_characters.equipped_skin_id has
    // ON DELETE SET NULL. Nothing extra to clean up.
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
