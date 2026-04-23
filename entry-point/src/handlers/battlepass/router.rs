use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;
use super::{current, progress, claim, unlock_premium};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/current", get(current::current_season))
        .route("/progress", get(progress::my_progress))
        .route("/claim/{tier}", post(claim::claim_tier))
        .route("/unlock-premium", post(unlock_premium::unlock_premium))
}
