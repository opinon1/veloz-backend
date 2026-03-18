use axum::{extract::State, Json, http::StatusCode};
use bcrypt::verify;
use serde::Deserialize;
use crate::state::AppState;
use crate::extractors::Claims;
use super::utils::delete_all_user_sessions;

#[derive(Deserialize)]
pub struct DeleteAccountRequest {
    pub password: String,
}

#[derive(sqlx::FromRow)]
struct PasswordRow {
    password_hash: String,
}

pub async fn delete_account(
    State(mut state): State<AppState>,
    Claims(session): Claims,
    Json(payload): Json<DeleteAccountRequest>,
) -> StatusCode {
    let row = sqlx::query_as::<_, PasswordRow>(
        "SELECT password_hash FROM users WHERE id = $1"
    )
    .bind(session.user_id)
    .fetch_optional(&state.db)
    .await;

    let row = match row {
        Ok(Some(r)) => r,
        _ => return StatusCode::INTERNAL_SERVER_ERROR,
    };

    let valid = verify(&payload.password, &row.password_hash).unwrap_or(false);
    if !valid {
        return StatusCode::UNAUTHORIZED;
    }

    let _ = delete_all_user_sessions(&mut state.redis, session.user_id).await;

    let result = sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(session.user_id)
        .execute(&state.db)
        .await;

    match result {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
