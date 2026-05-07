use axum::{extract::State, Json, http::StatusCode};
use serde::Serialize;
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::Claims;

#[derive(Serialize)]
pub struct CharacterView {
    pub id: Uuid,
    pub unlocked: bool,
    pub equipped_skin: Option<Uuid>,
    pub related_skins: Vec<Uuid>,
}

#[derive(sqlx::FromRow)]
struct Row {
    id: Uuid,
    unlocked: Option<bool>,
    equipped_skin_id: Option<Uuid>,
    default_unlocked: bool,
}

#[derive(sqlx::FromRow)]
struct SkinIdRow {
    id: Uuid,
    character_id: Uuid,
}

/// Return every active character with the caller's per-user state attached:
///
///   id              — character UUID
///   unlocked        — true iff the user has the character unlocked (either
///                     `user_characters.unlocked = true` or the character is
///                     `default_unlocked = true`)
///   equipped_skin   — the user's equipped skin for this character, or null
///   related_skins   — every active skin assigned to this character
pub async fn list_characters(
    State(state): State<AppState>,
    Claims(session): Claims,
) -> Result<Json<Vec<CharacterView>>, StatusCode> {
    let chars = sqlx::query_as::<_, Row>(
        r#"
        SELECT
            c.id,
            uc.unlocked,
            uc.equipped_skin_id,
            c.default_unlocked
        FROM characters c
        LEFT JOIN user_characters uc
            ON uc.character_id = c.id AND uc.user_id = $1
        WHERE c.is_active = TRUE
        ORDER BY c.created_at ASC
        "#,
    )
    .bind(session.user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let skins = sqlx::query_as::<_, SkinIdRow>(
        r#"
        SELECT id, character_id
        FROM skins
        WHERE is_active = TRUE
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut by_char: std::collections::HashMap<Uuid, Vec<Uuid>> =
        std::collections::HashMap::new();
    for s in skins {
        by_char.entry(s.character_id).or_default().push(s.id);
    }

    let out = chars
        .into_iter()
        .map(|c| CharacterView {
            id: c.id,
            unlocked: c.unlocked.unwrap_or(c.default_unlocked),
            equipped_skin: c.equipped_skin_id,
            related_skins: by_char.remove(&c.id).unwrap_or_default(),
        })
        .collect();

    Ok(Json(out))
}
