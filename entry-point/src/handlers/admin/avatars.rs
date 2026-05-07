use axum::{extract::{Path, State}, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::AdminClaims;
use crate::handlers::wallet::utils::is_valid_currency;

#[derive(Deserialize)]
pub struct CreateAvatarRequest {
    pub name: String,
    #[serde(default)]
    pub price: i64,
    #[serde(default = "default_currency")]
    pub currency: String,
}
fn default_currency() -> String { "soft".into() }

#[derive(Serialize, sqlx::FromRow)]
pub struct AvatarRow {
    pub id: Uuid,
    pub name: String,
    pub price: i64,
    pub currency: String,
    pub is_active: bool,
}

pub async fn create_avatar(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Json(payload): Json<CreateAvatarRequest>,
) -> Result<(StatusCode, Json<AvatarRow>), StatusCode> {
    if payload.name.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    if payload.price < 0 {
        return Err(StatusCode::BAD_REQUEST);
    }
    if !is_valid_currency(&payload.currency) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let row = sqlx::query_as::<_, AvatarRow>(
        r#"
        INSERT INTO avatars (name, price, currency)
        VALUES ($1, $2, $3)
        RETURNING id, name, price, currency, is_active
        "#,
    )
    .bind(&payload.name)
    .bind(payload.price)
    .bind(&payload.currency)
    .fetch_one(&state.db)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(db) if db.code().as_deref() == Some("23505") => StatusCode::CONFLICT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    })?;

    Ok((StatusCode::CREATED, Json(row)))
}

pub async fn list_all_avatars(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
) -> Result<Json<Vec<AvatarRow>>, StatusCode> {
    let rows = sqlx::query_as::<_, AvatarRow>(
        "SELECT id, name, price, currency, is_active FROM avatars ORDER BY created_at DESC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct UpdateAvatarRequest {
    pub name: Option<String>,
    pub price: Option<i64>,
    pub currency: Option<String>,
    pub is_active: Option<bool>,
}

pub async fn update_avatar(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateAvatarRequest>,
) -> Result<Json<AvatarRow>, StatusCode> {
    if let Some(p) = payload.price {
        if p < 0 {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    if let Some(ref c) = payload.currency {
        if !is_valid_currency(c) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    if let Some(ref n) = payload.name {
        if n.trim().is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    let row = sqlx::query_as::<_, AvatarRow>(
        r#"
        UPDATE avatars SET
            name       = COALESCE($2, name),
            price      = COALESCE($3, price),
            currency   = COALESCE($4, currency),
            is_active  = COALESCE($5, is_active),
            updated_at = CURRENT_TIMESTAMP
        WHERE id = $1
        RETURNING id, name, price, currency, is_active
        "#,
    )
    .bind(id)
    .bind(&payload.name)
    .bind(payload.price)
    .bind(&payload.currency)
    .bind(payload.is_active)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(db) if db.code().as_deref() == Some("23505") => StatusCode::CONFLICT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(row))
}

pub async fn delete_avatar(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let result = sqlx::query("DELETE FROM avatars WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}
