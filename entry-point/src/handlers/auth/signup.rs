use axum::{extract::State, Json, http::StatusCode};
use bcrypt::{hash, DEFAULT_COST};
use serde::{Deserialize, Serialize};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct SignupRequest {
    pub username: String,
    pub email: String,
    pub password: String,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct SignupResponse {
    pub id: uuid::Uuid,
    pub username: String,
    pub email: String,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub message: String,
}

pub async fn signup(
    State(state): State<AppState>,
    Json(payload): Json<SignupRequest>,
) -> Result<(StatusCode, Json<SignupResponse>), (StatusCode, Json<ErrorResponse>)> {
    // Validate password
    if payload.password.len() < 8 || payload.password.len() > 72 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { message: "Password must be between 8 and 72 characters".into() }),
        ));
    }

    let has_digit = payload.password.chars().any(|c| c.is_ascii_digit());
    let has_special = payload.password.chars().any(|c| !c.is_alphanumeric());
    if !has_digit && !has_special {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { message: "Password must contain at least one number or special character".into() }),
        ));
    }

    // Validate username
    let username_regex = regex::Regex::new(r"^[a-zA-Z0-9_]{3,30}$").unwrap();
    if !username_regex.is_match(&payload.username) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { message: "Username must be 3-30 characters and alphanumeric (underscores allowed)".into() }),
        ));
    }

    // Validate email
    if !payload.email.contains('@') || payload.email.len() < 5 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { message: "Invalid email address".into() }),
        ));
    }

    // Normalize email: lowercase so later signin lookups are case-insensitive.
    let email = payload.email.trim().to_lowercase();

    let password_hash = hash(payload.password, DEFAULT_COST)
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { message: "Internal server error".into() })))?;

    let user = sqlx::query_as::<_, SignupResponse>(
        r#"
        INSERT INTO users (username, email, password_hash)
        VALUES ($1, $2, $3)
        RETURNING id, username, email
        "#,
    )
    .bind(&payload.username)
    .bind(&email)
    .bind(password_hash)
    .fetch_one(&state.db)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(db_err) if db_err.code().as_deref() == Some("23505") => {
            (StatusCode::CONFLICT, Json(ErrorResponse { message: "User with this email or username already exists".into() }))
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { message: "Internal server error".into() })),
    })?;

    Ok((StatusCode::CREATED, Json(user)))
}
