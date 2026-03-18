use axum::{extract::State, Json, http::StatusCode};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::state::AppState;
use crate::models::auth::SessionData;
use super::utils::delete_all_user_sessions;

#[derive(sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    username: String,
    email: String,
}

#[derive(Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Serialize)]
pub struct RefreshResponse {
    pub access_token: String,
    pub refresh_token: String,
}

pub async fn refresh(
    State(mut state): State<AppState>,
    Json(payload): Json<RefreshRequest>,
) -> Result<Json<RefreshResponse>, StatusCode> {
    // Theft detection: if this refresh token was already rotated, it's a replay attack.
    // Invalidate all sessions for the user immediately.
    let tombstone_key = format!("revoked_refresh:{}", payload.refresh_token);
    let revoked_user: Option<String> = state.redis
        .get(&tombstone_key)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Some(user_id_str) = revoked_user {
        if let Ok(user_id) = Uuid::parse_str(&user_id_str) {
            let _ = delete_all_user_sessions(&mut state.redis, user_id).await;
        }
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Look up refresh token in Redis
    let session_json: Option<String> = state.redis
        .get(format!("refresh_token:{}", payload.refresh_token))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let session_json = session_json.ok_or(StatusCode::UNAUTHORIZED)?;

    let old_session: SessionData = serde_json::from_str(&session_json)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    // Delete old access token (using the correct linked field)
    let _: () = state.redis
        .del(format!("access_token:{}", old_session.associated_access_token))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Delete old refresh token
    let _: () = state.redis
        .del(format!("refresh_token:{}", payload.refresh_token))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Leave a short-lived tombstone so replays of this refresh token are detected
    let _: () = state.redis
        .set_ex(&tombstone_key, old_session.user_id.to_string(), 300u64)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Fetch fresh user data from Postgres
    let user = sqlx::query_as::<_, UserRow>(
        "SELECT id, username, email FROM users WHERE id = $1"
    )
    .bind(old_session.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::UNAUTHORIZED)?;

    // Generate new tokens
    let new_access = Uuid::new_v4().to_string();
    let new_refresh = Uuid::new_v4().to_string();

    // Preserve metadata from the old session so device/IP info is retained across refreshes
    let new_session = SessionData {
        user_id: user.id,
        username: user.username,
        email: user.email,
        associated_access_token: new_access.clone(),
        associated_refresh_token: new_refresh.clone(),
        created_at: old_session.created_at,
        user_agent: old_session.user_agent,
        ip: old_session.ip,
    };
    let new_json = serde_json::to_string(&new_session)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let _: () = state.redis
        .set_ex(format!("access_token:{}", new_access), &new_json, 900)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let _: () = state.redis
        .set_ex(format!("refresh_token:{}", new_refresh), &new_json, 604800)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Update user sessions set: remove old tokens, add new ones
    let session_set_key = format!("user_sessions:{}", user.id);
    let _: () = state.redis
        .srem(&session_set_key, vec![
            format!("access_token:{}", old_session.associated_access_token),
            format!("refresh_token:{}", payload.refresh_token),
        ])
        .await
        .unwrap_or(());
    let _: () = state.redis
        .sadd(&session_set_key, vec![
            format!("access_token:{}", new_access),
            format!("refresh_token:{}", new_refresh),
        ])
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let _: () = state.redis
        .expire(&session_set_key, 691200i64)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(RefreshResponse {
        access_token: new_access,
        refresh_token: new_refresh,
    }))
}
