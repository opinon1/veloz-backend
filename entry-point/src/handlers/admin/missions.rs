//! Admin CRUD for missions.

use crate::extractors::AdminClaims;
use crate::models::mission_types::{MissionCycle, MissionTriggerEvent};
use crate::state::AppState;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonV;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Serialize, sqlx::FromRow)]
pub struct MissionRow {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub cycle: String,
    pub trigger_event: String,
    pub target: JsonV,
    pub xp_reward: i64,
    pub is_active: bool,
}

#[derive(Deserialize)]
pub struct CreateMissionRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub cycle: String,
    pub trigger_event: String,
    pub target: JsonV,
    pub xp_reward: i64,
    #[serde(default = "default_true")]
    pub is_active: bool,
}

fn default_true() -> bool {
    true
}

fn validate_target(event: MissionTriggerEvent, target: &JsonV) -> Result<(), StatusCode> {
    if !target.is_object() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let amt = target.get("amount").and_then(|v| v.as_i64());
    match event {
        MissionTriggerEvent::RunCompleted => {
            // Requires {amount: > 0}
            if amt.is_none_or(|a| a <= 0) {
                return Err(StatusCode::BAD_REQUEST);
            }
        }
        MissionTriggerEvent::CurrencyCollected => {
            let cur = target
                .get("currency")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !matches!(cur, "high" | "soft" | "energy") {
                return Err(StatusCode::BAD_REQUEST);
            }
            if amt.is_none_or(|a| a <= 0) {
                return Err(StatusCode::BAD_REQUEST);
            }
        }
        MissionTriggerEvent::StorePurchase => {
            // item_type optional; amount required.
            if amt.is_none_or(|a| a <= 0) {
                return Err(StatusCode::BAD_REQUEST);
            }
            if let Some(it) = target.get("item_type").and_then(|v| v.as_str()) {
                if crate::models::store_types::ItemType::from_str(it).is_err() {
                    return Err(StatusCode::BAD_REQUEST);
                }
            }
        }
        MissionTriggerEvent::CharacterLevelUp => {
            let cid = target
                .get("character_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if Uuid::from_str(cid).is_err() {
                return Err(StatusCode::BAD_REQUEST);
            }
            let lvl = target.get("level").and_then(|v| v.as_i64()).unwrap_or(0);
            if !(1..=20).contains(&lvl) {
                return Err(StatusCode::BAD_REQUEST);
            }
        }
    }
    Ok(())
}

pub async fn create_mission(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Json(payload): Json<CreateMissionRequest>,
) -> Result<(StatusCode, Json<MissionRow>), StatusCode> {
    if payload.name.trim().is_empty() || payload.xp_reward <= 0 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let cycle = MissionCycle::from_str(&payload.cycle).map_err(|_| StatusCode::BAD_REQUEST)?;
    let trigger = MissionTriggerEvent::from_str(&payload.trigger_event)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    validate_target(trigger, &payload.target)?;

    let row = sqlx::query_as::<_, MissionRow>(
        r#"
        INSERT INTO missions (name, description, cycle, trigger_event, target, xp_reward, is_active)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id, name, description, cycle, trigger_event, target, xp_reward, is_active
        "#,
    )
    .bind(payload.name.trim())
    .bind(&payload.description)
    .bind(cycle.as_str())
    .bind(trigger.as_str())
    .bind(&payload.target)
    .bind(payload.xp_reward)
    .bind(payload.is_active)
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((StatusCode::CREATED, Json(row)))
}

pub async fn list_all_missions(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
) -> Result<Json<Vec<MissionRow>>, StatusCode> {
    let rows = sqlx::query_as::<_, MissionRow>(
        "SELECT id, name, description, cycle, trigger_event, target, xp_reward, is_active FROM missions ORDER BY created_at DESC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct UpdateMissionRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub cycle: Option<String>,
    pub trigger_event: Option<String>,
    pub target: Option<JsonV>,
    pub xp_reward: Option<i64>,
    pub is_active: Option<bool>,
}

pub async fn update_mission(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateMissionRequest>,
) -> Result<Json<MissionRow>, StatusCode> {
    if let Some(ref n) = payload.name {
        if n.trim().is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    if let Some(x) = payload.xp_reward {
        if x <= 0 {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    let cycle_str = match payload.cycle.as_deref() {
        Some(c) => Some(
            MissionCycle::from_str(c)
                .map_err(|_| StatusCode::BAD_REQUEST)?
                .as_str()
                .to_string(),
        ),
        None => None,
    };
    let trigger_str = match payload.trigger_event.as_deref() {
        Some(t) => Some(
            MissionTriggerEvent::from_str(t)
                .map_err(|_| StatusCode::BAD_REQUEST)?
                .as_str()
                .to_string(),
        ),
        None => None,
    };

    // If target was updated, validate against the (possibly-updated) trigger.
    if let Some(ref target_v) = payload.target {
        // Find the effective trigger: payload's, else fall back to current.
        let effective_trigger_str = match &trigger_str {
            Some(s) => s.clone(),
            None => {
                let (curr,): (String,) =
                    sqlx::query_as("SELECT trigger_event FROM missions WHERE id = $1")
                        .bind(id)
                        .fetch_optional(&state.db)
                        .await
                        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                        .ok_or(StatusCode::NOT_FOUND)?;
                curr
            }
        };
        let effective_trigger = MissionTriggerEvent::from_str(&effective_trigger_str)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        validate_target(effective_trigger, target_v)?;
    }

    let row = sqlx::query_as::<_, MissionRow>(
        r#"
        UPDATE missions SET
            name          = COALESCE($2, name),
            description   = COALESCE($3, description),
            cycle         = COALESCE($4, cycle),
            trigger_event = COALESCE($5, trigger_event),
            target        = COALESCE($6, target),
            xp_reward     = COALESCE($7, xp_reward),
            is_active     = COALESCE($8, is_active),
            updated_at    = CURRENT_TIMESTAMP
        WHERE id = $1
        RETURNING id, name, description, cycle, trigger_event, target, xp_reward, is_active
        "#,
    )
    .bind(id)
    .bind(payload.name.as_deref().map(|s| s.trim()))
    .bind(&payload.description)
    .bind(&cycle_str)
    .bind(&trigger_str)
    .bind(&payload.target)
    .bind(payload.xp_reward)
    .bind(payload.is_active)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(row))
}

pub async fn delete_mission(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let result = sqlx::query("DELETE FROM missions WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}
