use axum::{extract::{ConnectInfo, State}, http::HeaderMap, Json, http::StatusCode};
use bcrypt::verify;
use chrono::Utc;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use uuid::Uuid;
use crate::state::AppState;
use crate::models::auth::SessionData;

#[derive(Deserialize)]
pub struct SigninRequest {
    pub email: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct SigninResponse {
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(sqlx::FromRow)]
struct UserAuth {
    id: Uuid,
    username: String,
    email: String,
    password_hash: String,
    is_active: bool,
}

pub async fn signin(
    State(mut state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(payload): Json<SigninRequest>,
) -> Result<Json<SigninResponse>, StatusCode> {
    // Mirror signup normalization so signin is case-insensitive on email.
    let email = payload.email.trim().to_lowercase();
    let user = sqlx::query_as::<_, UserAuth>(
        "SELECT id, username, email, password_hash, is_active FROM users WHERE email = $1"
    )
    .bind(&email)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::UNAUTHORIZED)?;

    let valid = verify(payload.password, &user.password_hash)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !valid {
        return Err(StatusCode::UNAUTHORIZED);
    }

    if !user.is_active {
        return Err(StatusCode::FORBIDDEN);
    }

    let ip = headers
        .get("X-Forwarded-For")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| addr.ip().to_string());

    let user_agent = headers
        .get("User-Agent")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let access_token = Uuid::new_v4().to_string();
    let refresh_token = Uuid::new_v4().to_string();

    let session_data = SessionData {
        user_id: user.id,
        username: user.username,
        email: user.email,
        associated_access_token: access_token.clone(),
        associated_refresh_token: refresh_token.clone(),
        created_at: Some(Utc::now()),
        user_agent,
        ip: Some(ip),
    };
    let session_json = serde_json::to_string(&session_data)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Access token: 15 minutes, Refresh token: 7 days
    let _: () = state.redis
        .set_ex(format!("access_token:{}", access_token), &session_json, 900)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let _: () = state.redis
        .set_ex(format!("refresh_token:{}", refresh_token), &session_json, 604800)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Track tokens per user for signout-all
    let session_set_key = format!("user_sessions:{}", user.id);
    let _: () = state.redis
        .sadd(&session_set_key, vec![
            format!("access_token:{}", access_token),
            format!("refresh_token:{}", refresh_token),
        ])
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let _: () = state.redis
        .expire(&session_set_key, 691200i64)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(SigninResponse {
        access_token,
        refresh_token,
    }))
}
