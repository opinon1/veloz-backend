use axum::{extract::{Path, State}, Json, http::StatusCode};
use serde::Serialize;
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::Claims;

#[derive(Serialize)]
pub struct EquipSkinResponse {
    pub avatar_url: String,
}

pub async fn equip_skin(
    State(state): State<AppState>,
    Claims(session): Claims,
    Path(skin_id): Path<Uuid>,
) -> Result<Json<EquipSkinResponse>, StatusCode> {
    // Must own the skin.
    let owned: Option<(Uuid,)> = sqlx::query_as(
        "SELECT user_id FROM user_skins WHERE user_id = $1 AND skin_id = $2",
    )
    .bind(session.user_id)
    .bind(skin_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if owned.is_none() {
        return Err(StatusCode::FORBIDDEN);
    }

    let avatar_url = skin_id.to_string();

    sqlx::query(
        "UPDATE profiles SET avatar_url = $2, updated_at = CURRENT_TIMESTAMP WHERE user_id = $1",
    )
    .bind(session.user_id)
    .bind(&avatar_url)
    .execute(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(EquipSkinResponse { avatar_url }))
}
