//! Operational stats dashboard.
//!
//! Serves a self-contained HTML page (`dashboard.html`) plus JSON endpoints for
//! charts and ad-hoc SELECTs. Every data query runs against the READ-ONLY stats
//! pool (`state.stats_db`), never the app pool — so nothing here can mutate
//! data. Writes on the page (the "admin actions" panel) call the existing
//! `/admin/*` endpoints directly from the browser; they never pass through here.
//!
//! Guardrails on arbitrary SQL (defense in depth):
//!   1. `stats_db` runs every session as `veloz_stats`, a NOLOGIN SELECT-only
//!      role (SET ROLE on connect; role created in migration 0023).
//!   2. Those sessions are pinned `default_transaction_read_only = on` with a
//!      10s `statement_timeout` (see main.rs).
//!   3. Every query is wrapped `SELECT row_to_json(t) FROM ( <sql> ) t LIMIT N`,
//!      which forces a single SELECT, rejects DDL/multi-statement at parse time,
//!      and caps the row count.
//!   4. All JSON endpoints require `AdminClaims`.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::Html,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

use crate::extractors::AdminClaims;
use crate::state::AppState;

/// Hard cap on rows returned by any dashboard query.
const ROW_CAP: i64 = 5000;

const VALID_CHART_TYPES: [&str; 5] = ["table", "line", "bar", "pie", "stat"];

// ─────────── Dashboard page (public shell) ───────────

/// Serves the dashboard HTML. Intentionally unauthenticated: it's a static JS
/// shell with no secrets. The page collects an admin token in the browser and
/// sends it as a Bearer header to the gated JSON endpoints below.
pub async fn serve_dashboard() -> Html<&'static str> {
    Html(include_str!("dashboard.html"))
}

// ─────────── Chart CRUD (app pool) ───────────

#[derive(Serialize, sqlx::FromRow)]
pub struct ChartRow {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub sql: String,
    pub chart_type: String,
    pub config: Value,
    pub is_builtin: bool,
    pub sort_order: i32,
}

pub async fn list_charts(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
) -> Result<Json<Vec<ChartRow>>, StatusCode> {
    let rows = sqlx::query_as::<_, ChartRow>(
        "SELECT id, title, description, sql, chart_type, config, is_builtin, sort_order \
         FROM dashboard_charts ORDER BY sort_order, created_at",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct NewChart {
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub sql: String,
    #[serde(default = "default_chart_type")]
    pub chart_type: String,
    #[serde(default = "default_config")]
    pub config: Value,
    #[serde(default)]
    pub sort_order: i32,
}
fn default_chart_type() -> String {
    "table".to_string()
}
fn default_config() -> Value {
    Value::Object(Default::default())
}

pub async fn create_chart(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Json(body): Json<NewChart>,
) -> Result<(StatusCode, Json<ChartRow>), (StatusCode, String)> {
    let title = body.title.trim();
    let sql = body.sql.trim();
    if title.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "title is required".into()));
    }
    if sql.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "sql is required".into()));
    }
    if !VALID_CHART_TYPES.contains(&body.chart_type.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("chart_type must be one of {VALID_CHART_TYPES:?}"),
        ));
    }

    // Validate the SQL actually runs (read-only) before persisting it, so a
    // saved chart never silently 500s later.
    run_select(&state, sql).await?;

    let row = sqlx::query_as::<_, ChartRow>(
        "INSERT INTO dashboard_charts (title, description, sql, chart_type, config, sort_order) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         RETURNING id, title, description, sql, chart_type, config, is_builtin, sort_order",
    )
    .bind(title)
    .bind(body.description.trim())
    .bind(sql)
    .bind(&body.chart_type)
    .bind(&body.config)
    .bind(body.sort_order)
    .fetch_one(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok((StatusCode::CREATED, Json(row)))
}

pub async fn delete_chart(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    let res = sqlx::query("DELETE FROM dashboard_charts WHERE id = $1 AND is_builtin = FALSE")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if res.rows_affected() == 0 {
        // Either no such chart, or it's a built-in (not deletable).
        return Err((
            StatusCode::CONFLICT,
            "chart not found or is a built-in (cannot delete)".into(),
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}

// ─────────── Query execution (read-only stats pool) ───────────

#[derive(Serialize)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Value>,
    pub row_count: usize,
}

/// Run a saved chart's SQL and return its rows.
pub async fn chart_data(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Path(id): Path<Uuid>,
) -> Result<Json<QueryResult>, (StatusCode, String)> {
    let sql: Option<(String,)> = sqlx::query_as("SELECT sql FROM dashboard_charts WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let sql = sql
        .ok_or((StatusCode::NOT_FOUND, "chart not found".into()))?
        .0;

    let result = run_select(&state, &sql).await?;
    Ok(Json(result))
}

#[derive(Deserialize)]
pub struct QueryReq {
    pub sql: String,
}

/// Run an ad-hoc SELECT typed in the dashboard's query box.
pub async fn run_query(
    State(state): State<AppState>,
    AdminClaims(_): AdminClaims,
    Json(body): Json<QueryReq>,
) -> Result<Json<QueryResult>, (StatusCode, String)> {
    let result = run_select(&state, &body.sql).await?;
    Ok(Json(result))
}

/// Execute a single SELECT against the read-only stats pool and shape the result
/// into `{columns, rows}`. Wrapping in `row_to_json(t) FROM ( <sql> ) t` forces a
/// single SELECT (DDL/multi-statement become parse errors) and the outer LIMIT
/// caps rows. DB errors surface verbatim — admins are trusted and need them to
/// debug their SQL.
async fn run_select(state: &AppState, raw_sql: &str) -> Result<QueryResult, (StatusCode, String)> {
    let pool = state.stats_db.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "stats dashboard unavailable (read-only pool not connected)".into(),
    ))?;

    // Strip a trailing semicolon (common when pasting), then reject any further
    // semicolon as a multi-statement attempt. The subquery wrap below would also
    // reject it at parse time; this gives a clearer error.
    let trimmed = raw_sql.trim().trim_end_matches(';').trim();
    if trimmed.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "sql is empty".into()));
    }
    if trimmed.contains(';') {
        return Err((
            StatusCode::BAD_REQUEST,
            "only a single SELECT statement is allowed".into(),
        ));
    }

    let wrapped = format!("SELECT row_to_json(t) AS row FROM ( {trimmed} ) AS t LIMIT {ROW_CAP}");

    let rows = sqlx::query(&wrapped)
        .fetch_all(pool)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, db_error_message(&e)))?;

    let mut out = Vec::with_capacity(rows.len());
    let mut columns: Vec<String> = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        let v: Value = row
            .try_get("row")
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if i == 0
            && let Value::Object(map) = &v
        {
            columns = map.keys().cloned().collect();
        }
        out.push(v);
    }

    Ok(QueryResult {
        row_count: out.len(),
        columns,
        rows: out,
    })
}

/// Pull the human-readable message out of a SQLx error (the Postgres `DbError`
/// message when available), so the dashboard shows "column foo does not exist"
/// instead of an opaque wrapper.
fn db_error_message(e: &sqlx::Error) -> String {
    match e {
        sqlx::Error::Database(db) => db.message().to_string(),
        other => other.to_string(),
    }
}
