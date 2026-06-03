use axum::Router;
use axum::routing::post;

use crate::state::AppState;
use super::backfill;

pub fn router() -> Router<AppState> {
    Router::new().route("/backfill", post(backfill::backfill))
}
