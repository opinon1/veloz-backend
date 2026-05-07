use axum::{extract::{Path, State}, Json, http::StatusCode};
use serde::Serialize;
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::Claims;

#[derive(Serialize)]
pub struct EquipSkinResponse {
    pub character_id: Uuid,
    pub equipped_skin_id: Uuid,
}

/// Equip a skin the user owns.
///
/// Sets `user_characters.equipped_skin_id` for the skin's `character_id` so
/// subsequent `GET /characters` reflects the equipped skin per character.
/// The user must (a) own the skin and (b) have the character unlocked.
pub async fn equip_skin(
    State(state): State<AppState>,
    Claims(session): Claims,
    Path(skin_id): Path<Uuid>,
) -> Result<Json<EquipSkinResponse>, StatusCode> {
    let mut tx = state.db.begin().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Resolve the skin's character.
    let row: Option<(Uuid,)> = sqlx::query_as("SELECT character_id FROM skins WHERE id = $1")
        .bind(skin_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let (character_id,) = row.ok_or(StatusCode::NOT_FOUND)?;

    // Must own the skin.
    let owned: Option<(Uuid,)> = sqlx::query_as(
        "SELECT user_id FROM user_skins WHERE user_id = $1 AND skin_id = $2",
    )
    .bind(session.user_id)
    .bind(skin_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if owned.is_none() {
        return Err(StatusCode::FORBIDDEN);
    }

    // Upsert the (user, character) row with the new equipped skin. Unlock
    // here too so equipping a skin you own implies the character is yours.
    sqlx::query(
        r#"
        INSERT INTO user_characters (user_id, character_id, unlocked, equipped_skin_id)
        VALUES ($1, $2, TRUE, $3)
        ON CONFLICT (user_id, character_id) DO UPDATE SET
            unlocked = TRUE,
            equipped_skin_id = EXCLUDED.equipped_skin_id
        "#,
    )
    .bind(session.user_id)
    .bind(character_id)
    .bind(skin_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(EquipSkinResponse {
        character_id,
        equipped_skin_id: skin_id,
    }))
}
