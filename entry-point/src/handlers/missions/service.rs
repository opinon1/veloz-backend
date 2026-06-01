//! Mission tracking engine.
//!
//! `record_event` is called by handlers when a triggering action
//! happens (run completed, currency credited, store item bought,
//! character leveled up). It walks the active missions matching that
//! event, bumps the user's progress row for the current cycle, and
//! grants XP the moment progress reaches the target.
//!
//! All work happens inside the caller's transaction so a partial
//! credit can't survive a rolled-back business action.

use axum::http::StatusCode;
use chrono::Utc;
use serde_json::Value as Json;
use sqlx::{Postgres, Transaction};
use std::str::FromStr;
use uuid::Uuid;

use crate::leveling::level_from_total_xp;
use crate::models::mission_types::{MissionCycle, MissionTriggerEvent};

/// Every observable thing a handler can report. New variants here also
/// need a CHECK constraint entry on the `missions.trigger_event` column.
///
/// `CharacterLevelUp` exists for the level-up flow (cards system) which
/// is deferred — admin can already author missions targeting it.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum MissionEvent {
    RunCompleted,
    CurrencyCollected { currency: String, amount: i64 },
    StorePurchase { item_type: String },
    CharacterLevelUp { character_id: Uuid, level: i32 },
}

impl MissionEvent {
    fn trigger(&self) -> MissionTriggerEvent {
        match self {
            MissionEvent::RunCompleted => MissionTriggerEvent::RunCompleted,
            MissionEvent::CurrencyCollected { .. } => MissionTriggerEvent::CurrencyCollected,
            MissionEvent::StorePurchase { .. } => MissionTriggerEvent::StorePurchase,
            MissionEvent::CharacterLevelUp { .. } => MissionTriggerEvent::CharacterLevelUp,
        }
    }
}

/// Returns true if the event matches the mission's target shape. Targets
/// look like:
///   run_completed       => {"amount": N}                    (delta = 1 per call)
///   currency_collected  => {"currency":"soft","amount":N}   (delta = event amount)
///   store_purchase      => {"item_type":"...","amount":N}   (delta = 1 per matching call)
///   character_level_up  => {"character_id":"<uuid>","level":N}  (delta = 1 when level >= N)
///
/// Returns (matched, delta).
fn match_event(event: &MissionEvent, target: &Json) -> (bool, i64) {
    match event {
        MissionEvent::RunCompleted => (true, 1),

        MissionEvent::CurrencyCollected { currency, amount } => {
            let want = target.get("currency").and_then(|v| v.as_str());
            match want {
                Some(c) if c == currency => (true, *amount),
                _ => (false, 0),
            }
        }

        MissionEvent::StorePurchase { item_type } => {
            let want = target.get("item_type").and_then(|v| v.as_str());
            match want {
                // Empty / missing item_type filter = any purchase counts.
                None | Some("") => (true, 1),
                Some(t) if t == item_type => (true, 1),
                _ => (false, 0),
            }
        }

        MissionEvent::CharacterLevelUp {
            character_id,
            level,
        } => {
            let target_char = target.get("character_id").and_then(|v| v.as_str());
            let target_level = target.get("level").and_then(|v| v.as_i64()).unwrap_or(1);
            match target_char {
                Some(s) if Uuid::from_str(s).ok() == Some(*character_id) => {
                    if *level as i64 >= target_level {
                        (true, 1)
                    } else {
                        (false, 0)
                    }
                }
                _ => (false, 0),
            }
        }
    }
}

/// Returns the cap a mission's progress should saturate at. Once
/// progress >= cap we treat the mission as complete and credit XP.
fn target_cap(event: &MissionEvent, target: &Json) -> i64 {
    let _ = event;
    target
        .get("amount")
        .and_then(|v| v.as_i64())
        .unwrap_or(1)
        .max(1)
}

#[derive(sqlx::FromRow)]
struct MissionRow {
    id: Uuid,
    cycle: String,
    target: Json,
    xp_reward: i64,
}

pub async fn record_event(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    event: MissionEvent,
) -> Result<(), StatusCode> {
    let trigger = event.trigger().as_str();

    let missions = sqlx::query_as::<_, MissionRow>(
        "SELECT id, cycle, target, xp_reward FROM missions WHERE is_active = TRUE AND trigger_event = $1",
    )
    .bind(trigger)
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if missions.is_empty() {
        return Ok(());
    }

    let now = Utc::now();
    let mut total_xp_granted: i64 = 0;

    for m in missions {
        let (matched, delta) = match_event(&event, &m.target);
        if !matched || delta <= 0 {
            continue;
        }
        let cycle = match MissionCycle::from_str(&m.cycle) {
            Ok(c) => c,
            // Bad cycle in DB — skip rather than 500 mid-event.
            Err(_) => continue,
        };
        let cycle_key = cycle.cycle_key(now);
        let cap = target_cap(&event, &m.target);

        // Pre-check + insert + clamp + completion in one query so two
        // concurrent events can't double-credit.
        let row: Option<(
            i64,
            Option<chrono::DateTime<Utc>>,
            Option<chrono::DateTime<Utc>>,
        )> = sqlx::query_as(
            r#"
            INSERT INTO user_missions (user_id, mission_id, cycle_key, progress, updated_at)
            VALUES ($1, $2, $3, LEAST($4, $5), CURRENT_TIMESTAMP)
            ON CONFLICT (user_id, mission_id, cycle_key) DO UPDATE
                SET progress = LEAST(user_missions.progress + EXCLUDED.progress, $5),
                    updated_at = CURRENT_TIMESTAMP
            RETURNING
                progress,
                completed_at,
                CASE WHEN progress >= $5 AND completed_at IS NULL
                     THEN CURRENT_TIMESTAMP ELSE NULL END AS newly_completed
            "#,
        )
        .bind(user_id)
        .bind(m.id)
        .bind(&cycle_key)
        .bind(delta)
        .bind(cap)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let Some((_progress, completed_at, newly_completed)) = row else {
            continue;
        };

        // If progress reached cap on this call and completed_at was NULL,
        // stamp it and credit XP. The `newly_completed` flag we computed
        // above is a snapshot — re-check completed_at to be safe.
        if completed_at.is_none() && newly_completed.is_some() {
            sqlx::query(
                "UPDATE user_missions SET completed_at = CURRENT_TIMESTAMP WHERE user_id = $1 AND mission_id = $2 AND cycle_key = $3 AND completed_at IS NULL",
            )
            .bind(user_id)
            .bind(m.id)
            .bind(&cycle_key)
            .execute(&mut **tx)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            total_xp_granted += m.xp_reward;
        }
    }

    if total_xp_granted > 0 {
        // Update profile XP + recompute account_level. Same shape as
        // submit_run's update so derived state stays consistent.
        let (new_total_xp,): (i64,) = sqlx::query_as(
            r#"
            UPDATE profiles
            SET total_xp = total_xp + $2,
                updated_at = CURRENT_TIMESTAMP
            WHERE user_id = $1
            RETURNING total_xp
            "#,
        )
        .bind(user_id)
        .bind(total_xp_granted)
        .fetch_one(&mut **tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let new_level = level_from_total_xp(new_total_xp);
        sqlx::query("UPDATE profiles SET account_level = $2 WHERE user_id = $1")
            .bind(user_id)
            .bind(new_level)
            .execute(&mut **tx)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    Ok(())
}
