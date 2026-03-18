use axum::{extract::State, http::StatusCode};
use redis::AsyncCommands;
use crate::state::AppState;
use crate::extractors::Claims;

pub async fn signout(
    State(mut state): State<AppState>,
    Claims(session): Claims,
) -> StatusCode {
    let _: () = state.redis
        .del(vec![
            format!("access_token:{}", session.associated_access_token),
            format!("refresh_token:{}", session.associated_refresh_token),
        ])
        .await
        .unwrap_or(());

    let _: () = state.redis
        .srem(
            format!("user_sessions:{}", session.user_id),
            vec![
                format!("access_token:{}", session.associated_access_token),
                format!("refresh_token:{}", session.associated_refresh_token),
            ],
        )
        .await
        .unwrap_or(());

    StatusCode::NO_CONTENT
}
