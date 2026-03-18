use axum::{extract::State, Json, http::StatusCode};
use bcrypt::{hash, verify, DEFAULT_COST};
use serde::Deserialize;
use crate::state::AppState;
use crate::extractors::Claims;
use super::utils::delete_all_user_sessions;

#[derive(Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(sqlx::FromRow)]
struct PasswordRow {
    password_hash: String,
}

pub async fn change_password(
    State(mut state): State<AppState>,
    Claims(session): Claims,
    Json(payload): Json<ChangePasswordRequest>,
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

    let valid = verify(&payload.current_password, &row.password_hash).unwrap_or(false);
    if !valid {
        return StatusCode::UNAUTHORIZED;
    }

    if payload.new_password.len() < 8 || payload.new_password.len() > 72 {
        return StatusCode::BAD_REQUEST;
    }
    let has_digit = payload.new_password.chars().any(|c| c.is_ascii_digit());
    let has_special = payload.new_password.chars().any(|c| !c.is_alphanumeric());
    if !has_digit && !has_special {
        return StatusCode::BAD_REQUEST;
    }

    let new_hash = match hash(&payload.new_password, DEFAULT_COST) {
        Ok(h) => h,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
    };

    let result = sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
        .bind(new_hash)
        .bind(session.user_id)
        .execute(&state.db)
        .await;

    if result.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    let _ = delete_all_user_sessions(&mut state.redis, session.user_id).await;

    StatusCode::OK
}
