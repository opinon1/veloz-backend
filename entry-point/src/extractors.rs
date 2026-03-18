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
