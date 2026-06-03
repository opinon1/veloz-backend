//! Default-grant application. One pass per (user, default-row).
//!
//! Every INSERT uses ON CONFLICT DO NOTHING so re-running this for an
//! existing user is idempotent: skins/avatars/frames already owned
//! are skipped, characters already unlocked are skipped. Store-item
//! defaults are applied once per (user, item) — tracked via the
//! `default_grants_applied` table introduced here so the backfill
//! button can be hit repeatedly without re-crediting currency or
//! re-unlocking skins from the same store payload.

use axum::http::StatusCode;
use serde::Serialize;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::handlers::grants_util::apply_grant;
use crate::models::store_types::validate_grants;

#[derive(Debug, Default, Serialize, Clone, Copy)]
pub struct DefaultsApplied {
    pub skins: u32,
    pub avatars: u32,
    pub frames: u32,
    pub characters: u32,
    pub store_items: u32,
}

impl std::ops::AddAssign for DefaultsApplied {
    fn add_assign(&mut self, other: Self) {
        self.skins += other.skins;
        self.avatars += other.avatars;
        self.frames += other.frames;
        self.characters += other.characters;
        self.store_items += other.store_items;
    }
}

/// Apply every catalog `is_default` row to a single user inside the
/// caller's transaction. Idempotent.
pub async fn apply_defaults_for_user(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> Result<DefaultsApplied, StatusCode> {
    let mut out = DefaultsApplied::default();

    // ── Skins ──
    let skin_ids: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM skins WHERE is_default = TRUE AND is_active = TRUE",
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    for (skin_id,) in skin_ids {
        let res = sqlx::query(
            "INSERT INTO user_skins (user_id, skin_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(user_id)
        .bind(skin_id)
        .execute(&mut **tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if res.rows_affected() > 0 {
            out.skins += 1;
        }
    }

    // ── Avatars ──
    let avatar_ids: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM avatars WHERE is_default = TRUE AND is_active = TRUE",
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    for (avatar_id,) in avatar_ids {
        let res = sqlx::query(
            "INSERT INTO user_avatars (user_id, avatar_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(user_id)
        .bind(avatar_id)
        .execute(&mut **tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if res.rows_affected() > 0 {
            out.avatars += 1;
        }
    }

    // ── Frames ──
    let frame_ids: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM frames WHERE is_default = TRUE AND is_active = TRUE",
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    for (frame_id,) in frame_ids {
        let res = sqlx::query(
            "INSERT INTO user_frames (user_id, frame_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(user_id)
        .bind(frame_id)
        .execute(&mut **tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if res.rows_affected() > 0 {
            out.frames += 1;
        }
    }

    // ── Characters ──
    // characters.default_unlocked already makes the character render
    // unlocked in the user-facing list without a user_characters row.
    // We additionally write the row so concrete per-user state exists
    // (skin equip etc.). Idempotent via ON CONFLICT.
    let character_ids: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM characters WHERE default_unlocked = TRUE AND is_active = TRUE",
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    for (character_id,) in character_ids {
        let res = sqlx::query(
            r#"
            INSERT INTO user_characters (user_id, character_id, unlocked)
            VALUES ($1, $2, TRUE)
            ON CONFLICT (user_id, character_id) DO UPDATE
                SET unlocked = TRUE
                WHERE user_characters.unlocked = FALSE
            "#,
        )
        .bind(user_id)
        .bind(character_id)
        .execute(&mut **tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if res.rows_affected() > 0 {
            out.characters += 1;
        }
    }

    // ── Store items ──
    // Track per-(user, item) so the backfill button can re-run safely.
    let store_items: Vec<(Uuid, serde_json::Value)> = sqlx::query_as(
        "SELECT id, payload FROM store_items WHERE is_default = TRUE AND is_active = TRUE",
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    for (item_id, payload) in store_items {
        // Already applied? Skip.
        let inserted = sqlx::query(
            r#"
            INSERT INTO default_grants_applied (user_id, kind, ref_id)
            VALUES ($1, 'store_item', $2)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(user_id)
        .bind(item_id)
        .execute(&mut **tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if inserted.rows_affected() == 0 {
            continue;
        }
        // Apply payload. Skip the row entirely if the persisted
        // payload is malformed (admin should have been blocked at
        // create-time, but defensive).
        let grants = match validate_grants(&payload) {
            Ok(g) => g,
            Err(_) => continue,
        };
        for g in &grants {
            apply_grant(tx, user_id, g, "signup_default", item_id).await?;
        }
        out.store_items += 1;
    }

    Ok(out)
}
