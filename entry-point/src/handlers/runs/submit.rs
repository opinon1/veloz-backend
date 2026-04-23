use axum::{extract::State, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::Claims;
use crate::leveling::{xp_from_run, bp_xp_from_run, level_from_total_xp};
use crate::handlers::wallet::utils::adjust_balance;
use crate::handlers::battlepass::utils::active_season;

#[derive(Deserialize)]
pub struct SubmitRunRequest {
    pub score: i64,
    pub distance: i64,
    pub coins_collected: i64,
    pub duration_ms: i64,
}

#[derive(Serialize)]
pub struct SubmitRunResponse {
    pub run_id: Uuid,
    pub xp_awarded: i64,
    pub bp_xp_awarded: i64,
    pub new_total_xp: i64,
    pub new_level: i32,
    pub new_highscore: bool,
    pub main_highscore: i64,
    pub soft_awarded: i64,
    pub new_soft_balance: i64,
    pub active_season_id: Option<Uuid>,
}

pub async fn submit_run(
    State(state): State<AppState>,
    Claims(session): Claims,
    Json(payload): Json<SubmitRunRequest>,
) -> Result<Json<SubmitRunResponse>, StatusCode> {
    if payload.score < 0 || payload.distance < 0 || payload.coins_collected < 0 || payload.duration_ms < 0 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let xp_awarded = xp_from_run(payload.score);
    let bp_xp_awarded = bp_xp_from_run(payload.score);

    let mut tx = state.db.begin().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Insert run record.
    let run_id: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO runs (user_id, score, distance, coins_collected, duration_ms, xp_awarded, bp_xp_awarded)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id
        "#,
    )
    .bind(session.user_id)
    .bind(payload.score)
    .bind(payload.distance)
    .bind(payload.coins_collected)
    .bind(payload.duration_ms)
    .bind(xp_awarded)
    .bind(bp_xp_awarded)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Update profile: XP, level, highscore.
    let profile: (i64, i64) = sqlx::query_as(
        r#"
        UPDATE profiles
        SET
            total_xp = total_xp + $2,
            main_highscore = GREATEST(main_highscore, $3),
            updated_at = CURRENT_TIMESTAMP
        WHERE user_id = $1
        RETURNING total_xp, main_highscore
        "#,
    )
    .bind(session.user_id)
    .bind(xp_awarded)
    .bind(payload.score)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (new_total_xp, main_highscore) = profile;
    let new_level = level_from_total_xp(new_total_xp);

    sqlx::query("UPDATE profiles SET account_level = $2 WHERE user_id = $1")
        .bind(session.user_id)
        .bind(new_level)
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let new_highscore = payload.score > 0 && payload.score >= main_highscore;

    // Grant soft currency = coins_collected.
    let new_soft_balance = if payload.coins_collected > 0 {
        adjust_balance(
            &mut tx,
            session.user_id,
            "soft",
            payload.coins_collected,
            "run",
            Some(&run_id.0.to_string()),
        )
        .await?
    } else {
        sqlx::query_as::<_, (i64,)>("SELECT soft FROM wallets WHERE user_id = $1")
            .bind(session.user_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .0
    };

    // BP XP only during active season.
    let season = active_season(&state.db).await?;
    let active_season_id = if let Some(ref s) = season {
        sqlx::query(
            r#"
            INSERT INTO bp_progress (user_id, season_id, bp_xp)
            VALUES ($1, $2, $3)
            ON CONFLICT (user_id, season_id)
            DO UPDATE SET bp_xp = bp_progress.bp_xp + EXCLUDED.bp_xp,
                          updated_at = CURRENT_TIMESTAMP
            "#,
        )
        .bind(session.user_id)
        .bind(s.id)
        .bind(bp_xp_awarded)
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Some(s.id)
    } else {
        None
    };

    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(SubmitRunResponse {
        run_id: run_id.0,
        xp_awarded,
        bp_xp_awarded: if season.is_some() { bp_xp_awarded } else { 0 },
        new_total_xp,
        new_level,
        new_highscore,
        main_highscore,
        soft_awarded: payload.coins_collected,
        new_soft_balance,
        active_season_id,
    }))
}
