//! Whole-blob metadata endpoints. GET returns the full object; PUT
//! replaces it; DELETE clears it to `{}`. Backend never inspects the
//! contents — clients use this as a free-form k/v store.

use crate::extractors::Claims;
use crate::state::AppState;
use axum::{Json, extract::State, http::StatusCode};

/// Max serialized size of the metadata blob, in bytes. Keeps a single
/// user from stuffing the column with megabytes of junk.
pub const MAX_BLOB_BYTES: usize = 64 * 1024;

pub async fn get_blob(
    State(state): State<AppState>,
    Claims(session): Claims,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let row: Option<(serde_json::Value,)> =
        sqlx::query_as("SELECT data FROM user_metadata WHERE user_id = $1")
            .bind(session.user_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(row.map(|(v,)| v).unwrap_or_else(|| {
        serde_json::Value::Object(Default::default())
    })))
}

pub async fn put_blob(
    State(state): State<AppState>,
    Claims(session): Claims,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Top-level must be an object. Frontends can nest arbitrary types
    // inside, but the root needs to be a map so per-key endpoints work.
    if !payload.is_object() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let serialized = serde_json::to_vec(&payload).map_err(|_| StatusCode::BAD_REQUEST)?;
    if serialized.len() > MAX_BLOB_BYTES {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }

    let row: (serde_json::Value,) = sqlx::query_as(
        r#"
        INSERT INTO user_metadata (user_id, data, updated_at)
        VALUES ($1, $2, CURRENT_TIMESTAMP)
        ON CONFLICT (user_id) DO UPDATE
            SET data = EXCLUDED.data,
                updated_at = CURRENT_TIMESTAMP
        RETURNING data
        "#,
    )
    .bind(session.user_id)
    .bind(&payload)
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(row.0))
}

pub async fn delete_blob(
    State(state): State<AppState>,
    Claims(session): Claims,
) -> Result<StatusCode, StatusCode> {
    sqlx::query(
        "UPDATE user_metadata SET data = '{}'::jsonb, updated_at = CURRENT_TIMESTAMP WHERE user_id = $1",
    )
    .bind(session.user_id)
    .execute(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}
