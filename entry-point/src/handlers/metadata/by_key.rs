//! Per-key metadata endpoints. Sugar over the JSONB blob for clients
//! that only want to touch one entry at a time.

use super::blob::MAX_BLOB_BYTES;
use crate::extractors::Claims;
use crate::state::AppState;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;

/// Allowed key shape. Conservative on purpose: lowercase + digits +
/// underscore, 1..=64 chars. Matches the kinds of identifiers a UI
/// would use (`tutorial_step_3`, `audio_pref`).
fn is_valid_key(key: &str) -> bool {
    if key.is_empty() || key.len() > 64 {
        return false;
    }
    key.bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

#[derive(Deserialize)]
pub struct PutKeyRequest {
    pub value: serde_json::Value,
}

pub async fn get_key(
    State(state): State<AppState>,
    Claims(session): Claims,
    Path(key): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !is_valid_key(&key) {
        return Err(StatusCode::BAD_REQUEST);
    }
    // `data -> $2` returns SQL NULL when the key is missing. Decoding
    // SQL NULL straight into `Value` fails; wrap in `Option`.
    let row: Option<(Option<serde_json::Value>,)> =
        sqlx::query_as("SELECT data -> $2 FROM user_metadata WHERE user_id = $1")
            .bind(session.user_id)
            .bind(&key)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let value = row.and_then(|(v,)| v).unwrap_or(serde_json::Value::Null);
    if value.is_null() {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(Json(value))
}

pub async fn put_key(
    State(state): State<AppState>,
    Claims(session): Claims,
    Path(key): Path<String>,
    Json(payload): Json<PutKeyRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !is_valid_key(&key) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let existing: Option<(serde_json::Value,)> =
        sqlx::query_as("SELECT data FROM user_metadata WHERE user_id = $1 FOR UPDATE")
            .bind(session.user_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut data = existing
        .map(|(v,)| v)
        .unwrap_or_else(|| serde_json::Value::Object(Default::default()));

    // Existing row may legitimately be `null` if a previous put set the
    // root to null (the blob handler guards against this, but be safe).
    if !data.is_object() {
        data = serde_json::Value::Object(Default::default());
    }
    data.as_object_mut()
        .expect("checked above")
        .insert(key.clone(), payload.value);

    let serialized = serde_json::to_vec(&data).map_err(|_| StatusCode::BAD_REQUEST)?;
    if serialized.len() > MAX_BLOB_BYTES {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }

    let row: (Option<serde_json::Value>,) = sqlx::query_as(
        r#"
        INSERT INTO user_metadata (user_id, data, updated_at)
        VALUES ($1, $2, CURRENT_TIMESTAMP)
        ON CONFLICT (user_id) DO UPDATE
            SET data = EXCLUDED.data,
                updated_at = CURRENT_TIMESTAMP
        RETURNING data -> $3
        "#,
    )
    .bind(session.user_id)
    .bind(&data)
    .bind(&key)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(row.0.unwrap_or(serde_json::Value::Null)))
}

pub async fn delete_key(
    State(state): State<AppState>,
    Claims(session): Claims,
    Path(key): Path<String>,
) -> Result<StatusCode, StatusCode> {
    if !is_valid_key(&key) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let result = sqlx::query(
        "UPDATE user_metadata SET data = data - $2, updated_at = CURRENT_TIMESTAMP WHERE user_id = $1",
    )
    .bind(session.user_id)
    .bind(&key)
    .execute(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}
