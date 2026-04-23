use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;
use super::{submit, history, leaderboard};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(submit::submit_run).get(history::my_history))
        .route("/leaderboard", get(leaderboard::leaderboard))
}
