use axum::Json;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;
use crate::extractors::Claims;

#[derive(Serialize)]
pub struct VerifyResponse {
    pub user_id: Uuid,
    pub username: String,
    pub email: String,
    pub created_at: Option<DateTime<Utc>>,
}

pub async fn verify(Claims(session): Claims) -> Json<VerifyResponse> {
    Json(VerifyResponse {
        user_id: session.user_id,
        username: session.username,
        email: session.email,
        created_at: session.created_at,
    })
}
