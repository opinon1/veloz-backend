use axum::{extract::{Path, Query, State}, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::state::AppState;
use crate::extractors::AdminClaims;
use crate::handlers::wallet::utils::adjust_balance_oneshot;

// ─────────── List users ───────────

#[derive(Deserialize)]
pub struct ListUsersQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    pub search: Option<String>,
}
fn default_limit() -> i64 { 50 }

#[derive(Serialize, sqlx::FromRow)]
pub struct UserRow {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub role: String,
    pub is_active: bool,
    pub account_level: i32,
    pub total_xp: i64,
    pub main_highscore: i64,
    pub high: i64,
    pub soft: i64,
    pub energy: i64,
}

pub async fn list_users(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Query(q): Query<ListUsersQuery>,
) -> Result<Json<Vec<UserRow>>, StatusCode> {
    let limit = q.limit.clamp(1, 500);
    let search_pat = q.search.map(|s| format!("%{}%", s));

    let rows = sqlx::query_as::<_, UserRow>(
        r#"
        SELECT
            u.id, u.username, u.email, u.role, u.is_active,
            p.account_level, p.total_xp, p.main_highscore,
            w.high, w.soft, w.energy
        FROM users u
        JOIN profiles p ON p.user_id = u.id
        JOIN wallets  w ON w.user_id = u.id
        WHERE ($2::text IS NULL OR u.username ILIKE $2 OR u.email ILIKE $2)
        ORDER BY u.created_at DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .bind(search_pat)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(rows))
}

// ─────────── Role update ───────────

#[derive(Deserialize)]
pub struct UpdateRoleRequest {
    pub role: String,        // 'user' | 'admin'
}

#[derive(Serialize)]
pub struct UpdateRoleResponse {
    pub user_id: Uuid,
    pub role: String,
}

pub async fn update_role(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Path(user_id): Path<Uuid>,
    Json(payload): Json<UpdateRoleRequest>,
) -> Result<Json<UpdateRoleResponse>, StatusCode> {
    if payload.role != "user" && payload.role != "admin" {
        return Err(StatusCode::BAD_REQUEST);
    }

    let result = sqlx::query("UPDATE users SET role = $2 WHERE id = $1")
        .bind(user_id)
        .bind(&payload.role)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(Json(UpdateRoleResponse { user_id, role: payload.role }))
}

// ─────────── Currency grant ───────────

#[derive(Deserialize)]
pub struct GrantRequest {
    pub currency: String,    // 'high' | 'soft' | 'energy'
    pub amount: i64,         // may be negative to deduct
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Serialize)]
pub struct GrantResponse {
    pub user_id: Uuid,
    pub currency: String,
    pub delta: i64,
    pub new_balance: i64,
}

pub async fn grant_currency(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Path(user_id): Path<Uuid>,
    Json(payload): Json<GrantRequest>,
) -> Result<Json<GrantResponse>, StatusCode> {
    let reason = payload.reason.as_deref().unwrap_or("admin_grant");
    let new_balance = adjust_balance_oneshot(
        &state.db,
        user_id,
        &payload.currency,
        payload.amount,
        reason,
        None,
    )
    .await?;

    Ok(Json(GrantResponse {
        user_id,
        currency: payload.currency,
        delta: payload.amount,
        new_balance,
    }))
}

// ─────────── Profile override (price multiplier, highscore, etc.) ───────────

#[derive(Deserialize)]
pub struct UpdateProfileRequest {
    pub account_level: Option<i32>,
    pub total_xp: Option<i64>,
    pub price_multiplier: Option<f64>,
    pub main_highscore: Option<i64>,
    pub avatar_url: Option<String>,
    pub frame_url: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct ProfileRow {
    pub user_id: Uuid,
    pub account_level: i32,
    pub total_xp: i64,
    pub price_multiplier: f64,
    pub main_highscore: i64,
    pub avatar_url: Option<String>,
    pub frame_url: Option<String>,
}

pub async fn update_profile(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Path(user_id): Path<Uuid>,
    Json(payload): Json<UpdateProfileRequest>,
) -> Result<Json<ProfileRow>, StatusCode> {
    let row = sqlx::query_as::<_, ProfileRow>(
        r#"
        UPDATE profiles SET
            account_level    = COALESCE($2, account_level),
            total_xp         = COALESCE($3, total_xp),
            price_multiplier = COALESCE($4, price_multiplier),
            main_highscore   = COALESCE($5, main_highscore),
            avatar_url       = COALESCE($6, avatar_url),
            frame_url        = COALESCE($7, frame_url),
            updated_at       = CURRENT_TIMESTAMP
        WHERE user_id = $1
        RETURNING user_id, account_level, total_xp, price_multiplier, main_highscore, avatar_url, frame_url
        "#,
    )
    .bind(user_id)
    .bind(payload.account_level)
    .bind(payload.total_xp)
    .bind(payload.price_multiplier)
    .bind(payload.main_highscore)
    .bind(&payload.avatar_url)
    .bind(&payload.frame_url)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(row))
}
