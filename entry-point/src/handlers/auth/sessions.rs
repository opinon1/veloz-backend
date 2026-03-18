use axum::{extract::State, Json, http::StatusCode};
use chrono::{DateTime, Utc};
use redis::AsyncCommands;
use serde::Serialize;
use crate::state::AppState;
use crate::extractors::Claims;
use crate::models::auth::SessionData;

#[derive(Serialize)]
pub struct SessionInfo {
    pub created_at: Option<DateTime<Utc>>,
    pub user_agent: Option<String>,
    pub ip: Option<String>,
}

pub async fn sessions(
    State(mut state): State<AppState>,
    Claims(session): Claims,
) -> Result<Json<Vec<SessionInfo>>, StatusCode> {
    let token_keys: Vec<String> = state.redis
        .smembers(format!("user_sessions:{}", session.user_id))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut result = Vec::new();

    // Iterate refresh token keys — they represent the true session lifetime (7 days).
    // Access tokens expire after 15 min and would make active sessions disappear from the list.
    for key in token_keys.iter().filter(|k| k.starts_with("refresh_token:")) {
        let data: Option<String> = state.redis
            .get(key)
            .await
            .unwrap_or(None);

        if let Some(json) = data {
            if let Ok(s) = serde_json::from_str::<SessionData>(&json) {
                result.push(SessionInfo {
                    created_at: s.created_at,
                    user_agent: s.user_agent,
                    ip: s.ip,
                });
            }
        }
    }

    Ok(Json(result))
}
