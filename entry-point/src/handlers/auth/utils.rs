use axum::http::StatusCode;
use redis::AsyncCommands;
use uuid::Uuid;

pub async fn delete_all_user_sessions(
    redis: &mut redis::aio::ConnectionManager,
    user_id: Uuid,
) -> Result<(), StatusCode> {
    let session_keys: Vec<String> = redis
        .smembers(format!("user_sessions:{}", user_id))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !session_keys.is_empty() {
        let _: () = redis
            .del(session_keys)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    let _: () = redis
        .del(format!("user_sessions:{}", user_id))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(())
}
