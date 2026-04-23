use axum::{
    extract::{FromRequestParts, FromRef},
    http::{request::Parts, StatusCode},
};
use redis::AsyncCommands;
use crate::state::AppState;
use crate::models::auth::SessionData;

pub struct Claims(pub SessionData);

impl<S> FromRequestParts<S> for Claims
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let auth_header = parts.headers
            .get("Authorization")
            .and_then(|h| h.to_str().ok())
            .ok_or(StatusCode::UNAUTHORIZED)?;

        if !auth_header.starts_with("Bearer ") {
            return Err(StatusCode::UNAUTHORIZED);
        }

        let token = &auth_header[7..];

        let app_state = AppState::from_ref(state);
        let mut redis_conn = app_state.redis.clone();

        let session_json: Option<String> = redis_conn
            .get(format!("access_token:{}", token))
            .await
            .map_err(|_| StatusCode::UNAUTHORIZED)?;

        let session_json = session_json.ok_or(StatusCode::UNAUTHORIZED)?;

        let session_data: SessionData = serde_json::from_str(&session_json)
            .map_err(|_| StatusCode::UNAUTHORIZED)?;

        Ok(Claims(session_data))
    }
}

/// Admin-only extractor. Validates the Bearer token like `Claims`, then
/// checks the user's `role` column in Postgres. Rejects with 403 if not 'admin'.
pub struct AdminClaims(pub SessionData);

impl<S> FromRequestParts<S> for AdminClaims
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Claims(session) = Claims::from_request_parts(parts, state).await?;
        let app_state = AppState::from_ref(state);

        let role: Option<(String,)> = sqlx::query_as("SELECT role FROM users WHERE id = $1")
            .bind(session.user_id)
            .fetch_optional(&app_state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        match role {
            Some((r,)) if r == "admin" => Ok(AdminClaims(session)),
            _ => Err(StatusCode::FORBIDDEN),
        }
    }
}
