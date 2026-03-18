use axum::{extract::State, http::StatusCode};
use crate::state::AppState;
use crate::extractors::Claims;
use super::utils::delete_all_user_sessions;

pub async fn signout_all(
    State(mut state): State<AppState>,
    Claims(session): Claims,
) -> StatusCode {
    let _ = delete_all_user_sessions(&mut state.redis, session.user_id).await;
    StatusCode::NO_CONTENT
}
