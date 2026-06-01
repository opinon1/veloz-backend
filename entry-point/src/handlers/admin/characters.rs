use crate::extractors::AdminClaims;
use crate::models::rarity::Rarity;
use crate::state::AppState;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct CreateCharacterRequest {
    pub name: String,
    #[serde(default)]
    pub default_unlocked: bool,
    /// Free-form frontend metadata (animation hints, sort order, lore,
    /// etc). Stored as-is, not interpreted by the backend. Defaults to
    /// `{}` rather than serde's `Null` so the column matches the DB
    /// default.
    #[serde(default = "empty_object")]
    pub metadata: serde_json::Value,
    /// One of: common, uncommon, rare, epic, legendary. Default common.
    #[serde(default = "default_rarity")]
    pub rarity: String,
}

fn empty_object() -> serde_json::Value {
    serde_json::Value::Object(Default::default())
}

fn default_rarity() -> String {
    "common".to_string()
}

#[derive(Serialize, sqlx::FromRow)]
pub struct CharacterRow {
    pub id: Uuid,
    pub name: String,
    pub is_active: bool,
    pub default_unlocked: bool,
    pub metadata: serde_json::Value,
    pub rarity: String,
}

pub async fn create_character(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Json(payload): Json<CreateCharacterRequest>,
) -> Result<(StatusCode, Json<CharacterRow>), StatusCode> {
    if payload.name.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let rarity = Rarity::from_str(&payload.rarity).map_err(|_| StatusCode::BAD_REQUEST)?;

    let row = sqlx::query_as::<_, CharacterRow>(
        r#"
        INSERT INTO characters (name, default_unlocked, metadata, rarity)
        VALUES ($1, $2, $3, $4)
        RETURNING id, name, is_active, default_unlocked, metadata, rarity
        "#,
    )
    .bind(&payload.name)
    .bind(payload.default_unlocked)
    .bind(&payload.metadata)
    .bind(rarity.as_str())
    .fetch_one(&state.db)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(db) if db.code().as_deref() == Some("23505") => StatusCode::CONFLICT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    })?;

    Ok((StatusCode::CREATED, Json(row)))
}

pub async fn list_all_characters(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
) -> Result<Json<Vec<CharacterRow>>, StatusCode> {
    let rows = sqlx::query_as::<_, CharacterRow>(
        "SELECT id, name, is_active, default_unlocked, metadata, rarity FROM characters ORDER BY created_at DESC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct UpdateCharacterRequest {
    pub name: Option<String>,
    pub is_active: Option<bool>,
    pub default_unlocked: Option<bool>,
    pub metadata: Option<serde_json::Value>,
    pub rarity: Option<String>,
}

pub async fn update_character(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateCharacterRequest>,
) -> Result<Json<CharacterRow>, StatusCode> {
    if let Some(ref n) = payload.name {
        if n.trim().is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    let rarity_str = match payload.rarity.as_deref() {
        Some(r) => Some(
            Rarity::from_str(r)
                .map_err(|_| StatusCode::BAD_REQUEST)?
                .as_str()
                .to_string(),
        ),
        None => None,
    };

    let row = sqlx::query_as::<_, CharacterRow>(
        r#"
        UPDATE characters SET
            name             = COALESCE($2, name),
            is_active        = COALESCE($3, is_active),
            default_unlocked = COALESCE($4, default_unlocked),
            metadata         = COALESCE($5, metadata),
            rarity           = COALESCE($6, rarity),
            updated_at       = CURRENT_TIMESTAMP
        WHERE id = $1
        RETURNING id, name, is_active, default_unlocked, metadata, rarity
        "#,
    )
    .bind(id)
    .bind(&payload.name)
    .bind(payload.is_active)
    .bind(payload.default_unlocked)
    .bind(&payload.metadata)
    .bind(&rarity_str)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(db) if db.code().as_deref() == Some("23505") => StatusCode::CONFLICT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(row))
}

pub async fn delete_character(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let result = sqlx::query("DELETE FROM characters WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}
