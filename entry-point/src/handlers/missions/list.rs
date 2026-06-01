//! GET /missions — list active missions with caller's current-cycle
//! progress. cycle_key is computed server-side from cycle + now(UTC).

use crate::extractors::Claims;
use crate::models::mission_types::MissionCycle;
use crate::state::AppState;
use axum::{Json, extract::State, http::StatusCode};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value as JsonV;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Serialize)]
pub struct MissionView {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub cycle: String,
    pub trigger_event: String,
    pub target: JsonV,
    pub xp_reward: i64,
    pub progress: i64,
    pub target_amount: i64,
    pub completed_at: Option<DateTime<Utc>>,
    pub cycle_key: String,
}

#[derive(sqlx::FromRow)]
struct Row {
    id: Uuid,
    name: String,
    description: String,
    cycle: String,
    trigger_event: String,
    target: JsonV,
    xp_reward: i64,
}

pub async fn list_missions(
    State(state): State<AppState>,
    Claims(session): Claims,
) -> Result<Json<Vec<MissionView>>, StatusCode> {
    let rows = sqlx::query_as::<_, Row>(
        "SELECT id, name, description, cycle, trigger_event, target, xp_reward FROM missions WHERE is_active = TRUE ORDER BY created_at ASC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if rows.is_empty() {
        return Ok(Json(vec![]));
    }

    let now = Utc::now();
    let mut out = Vec::with_capacity(rows.len());

    for r in rows {
        let cycle =
            MissionCycle::from_str(&r.cycle).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let cycle_key = cycle.cycle_key(now);
        let target_amount = r
            .target
            .get("amount")
            .and_then(|v| v.as_i64())
            .unwrap_or(1)
            .max(1);

        let progress_row: Option<(i64, Option<DateTime<Utc>>)> = sqlx::query_as(
            "SELECT progress, completed_at FROM user_missions WHERE user_id = $1 AND mission_id = $2 AND cycle_key = $3",
        )
        .bind(session.user_id)
        .bind(r.id)
        .bind(&cycle_key)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let (progress, completed_at) = progress_row.unwrap_or((0, None));

        out.push(MissionView {
            id: r.id,
            name: r.name,
            description: r.description,
            cycle: r.cycle,
            trigger_event: r.trigger_event,
            target: r.target,
            xp_reward: r.xp_reward,
            progress,
            target_amount,
            completed_at,
            cycle_key,
        });
    }

    Ok(Json(out))
}
